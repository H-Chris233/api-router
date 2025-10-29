//! HTTP 客户端模块
//!
//! 提供基于 smol 的 HTTP/HTTPS 客户端实现，支持：
//! - HTTP/1.1 keep-alive 连接复用
//! - TLS/HTTPS 连接
//! - 连接池管理
//! - 流式响应（SSE）
//! - 反压和心跳机制

use crate::config::StreamConfig;
use crate::errors::{RouterError, RouterResult};
use crate::url_parser::Url;
use async_channel::{bounded, Receiver, Sender};
use async_tls::TlsConnector;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use smol::io::{AsyncReadExt, AsyncWriteExt};
use smol::net::TcpStream;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, trace, warn};

/// 连接池最大连接数
const DEFAULT_POOL_MAX_SIZE: usize = 10;
/// 连接池空闲超时时间（秒）
const DEFAULT_POOL_IDLE_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ConnectionKey {
    scheme: String,
    host: String,
    port: u16,
}

impl ConnectionKey {
    fn from_url(url: &Url) -> RouterResult<Self> {
        let scheme = url.scheme().to_string();
        let host = url
            .host_str()
            .ok_or_else(|| RouterError::Url("Invalid URL: missing host".to_string()))?
            .to_string();
        let port = url
            .port_or_known_default()
            .unwrap_or(if scheme == "https" { 443 } else { 80 });
        Ok(ConnectionKey { scheme, host, port })
    }
}

enum PooledStream {
    Tcp(TcpStream),
    Tls(async_tls::client::TlsStream<TcpStream>),
}

struct PooledConnection {
    stream: PooledStream,
    last_used: Instant,
    connection_id: u64,
}

impl PooledConnection {
    fn new(stream: PooledStream, connection_id: u64) -> Self {
        Self {
            stream,
            last_used: Instant::now(),
            connection_id,
        }
    }

    fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    fn is_expired(&self, idle_timeout: Duration) -> bool {
        self.last_used.elapsed() > idle_timeout
    }

    async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match &mut self.stream {
            PooledStream::Tcp(s) => s.write_all(buf).await,
            PooledStream::Tls(s) => s.write_all(buf).await,
        }
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        match &mut self.stream {
            PooledStream::Tcp(s) => s.flush().await,
            PooledStream::Tls(s) => s.flush().await,
        }
    }

    async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match &mut self.stream {
            PooledStream::Tcp(s) => s.read(buf).await,
            PooledStream::Tls(s) => s.read(buf).await,
        }
    }
}

#[derive(Clone)]
pub struct PoolConfig {
    pub max_size: usize,
    pub idle_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_size: DEFAULT_POOL_MAX_SIZE,
            idle_timeout: Duration::from_secs(DEFAULT_POOL_IDLE_TIMEOUT_SECS),
        }
    }
}

struct ConnectionPoolInner {
    sender: Sender<PooledConnection>,
    receiver: Receiver<PooledConnection>,
    config: PoolConfig,
    active_count: Arc<std::sync::atomic::AtomicUsize>,
    next_connection_id: Arc<std::sync::atomic::AtomicU64>,
}

impl ConnectionPoolInner {
    fn new(config: PoolConfig) -> Self {
        let (sender, receiver) = bounded(config.max_size);
        Self {
            sender,
            receiver,
            config,
            active_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            next_connection_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    async fn acquire(&self, key: &ConnectionKey) -> RouterResult<PooledConnection> {
        loop {
            match self.receiver.try_recv() {
                Ok(mut conn) => {
                    if conn.is_expired(self.config.idle_timeout) {
                        trace!(
                            connection_id = conn.connection_id,
                            "Connection expired, creating new one"
                        );
                        self.active_count
                            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                        continue;
                    }
                    conn.touch();
                    trace!(
                        connection_id = conn.connection_id,
                        "Reusing pooled connection"
                    );
                    return Ok(conn);
                }
                Err(_) => {
                    let current = self.active_count.load(std::sync::atomic::Ordering::Relaxed);
                    if current < self.config.max_size {
                        self.active_count
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let connection_id = self
                            .next_connection_id
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        trace!(connection_id = connection_id, "Creating new connection");
                        return self.create_connection(key, connection_id).await;
                    } else {
                        if let Ok(mut conn) = self.receiver.recv().await {
                            if conn.is_expired(self.config.idle_timeout) {
                                trace!(
                                    connection_id = conn.connection_id,
                                    "Connection expired after waiting, creating new one"
                                );
                                let connection_id = self
                                    .next_connection_id
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                return self.create_connection(key, connection_id).await;
                            }
                            conn.touch();
                            trace!(
                                connection_id = conn.connection_id,
                                "Reusing pooled connection after wait"
                            );
                            return Ok(conn);
                        } else {
                            return Err(RouterError::Io(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "Connection pool closed",
                            )));
                        }
                    }
                }
            }
        }
    }

    async fn create_connection(
        &self,
        key: &ConnectionKey,
        connection_id: u64,
    ) -> RouterResult<PooledConnection> {
        let tcp_stream = TcpStream::connect((&key.host[..], key.port)).await?;

        let stream = if key.scheme == "https" {
            let tls_connector = create_tls_connector();
            let tls_stream = tls_connector
                .connect(&key.host, tcp_stream)
                .await
                .map_err(|e| RouterError::Tls(e.to_string()))?;
            PooledStream::Tls(tls_stream)
        } else {
            PooledStream::Tcp(tcp_stream)
        };

        Ok(PooledConnection::new(stream, connection_id))
    }

    async fn return_connection(&self, conn: PooledConnection) {
        if self.sender.try_send(conn).is_err() {
            self.active_count
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            trace!("Connection pool full, dropping connection");
        }
    }

    fn recycle_connection(&self) {
        self.active_count
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

pub struct ConnectionPool {
    pools: DashMap<ConnectionKey, Arc<ConnectionPoolInner>>,
    config: PoolConfig,
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            pools: DashMap::new(),
            config,
        }
    }

    fn get_pool(&self, key: &ConnectionKey) -> Arc<ConnectionPoolInner> {
        self.pools
            .entry(key.clone())
            .or_insert_with(|| Arc::new(ConnectionPoolInner::new(self.config.clone())))
            .clone()
    }

    async fn acquire(&self, key: &ConnectionKey) -> RouterResult<PooledConnection> {
        let pool = self.get_pool(key);
        pool.acquire(key).await
    }

    async fn return_connection(&self, key: &ConnectionKey, conn: PooledConnection) {
        let pool = self.get_pool(key);
        pool.return_connection(conn).await;
    }

    fn recycle_connection(&self, key: &ConnectionKey) {
        if let Some(pool) = self.pools.get(key) {
            pool.recycle_connection();
        }
    }
}

static CONNECTION_POOL: Lazy<ConnectionPool> =
    Lazy::new(|| ConnectionPool::new(PoolConfig::default()));

fn create_tls_connector() -> TlsConnector {
    TlsConnector::new()
}

fn build_request_bytes(
    method: &str,
    path: &str,
    host: &str,
    headers: &HashMap<String, String>,
    body: Option<&[u8]>,
) -> Vec<u8> {
    let mut request = Vec::new();
    request.extend_from_slice(format!("{} {} HTTP/1.1\r\n", method, path).as_bytes());
    request.extend_from_slice(format!("Host: {}\r\n", host).as_bytes());
    request.extend_from_slice(b"Connection: keep-alive\r\n");

    for (key, value) in headers {
        request.extend_from_slice(format!("{}: {}\r\n", key, value).as_bytes());
    }

    if let Some(body_bytes) = body {
        request.extend_from_slice(format!("Content-Length: {}\r\n", body_bytes.len()).as_bytes());
        request.extend_from_slice(b"\r\n");
        request.extend_from_slice(body_bytes);
    } else {
        request.extend_from_slice(b"\r\n");
    }

    request
}

fn parse_http_response(response: &[u8]) -> RouterResult<(usize, HashMap<String, String>, usize)> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| {
            RouterError::Upstream("Invalid HTTP response: no header separator".to_string())
        })?;

    let headers_str = std::str::from_utf8(&response[..header_end])
        .map_err(|_| RouterError::Upstream("Invalid UTF-8 in response headers".to_string()))?;

    let mut lines = headers_str.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| RouterError::Upstream("Empty HTTP response".to_string()))?;

    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<usize>().ok())
        .ok_or_else(|| RouterError::Upstream("Invalid status code".to_string()))?;

    let mut headers = HashMap::new();
    for line in lines {
        if let Some(pos) = line.find(':') {
            let key = line[..pos].trim().to_lowercase();
            let value = line[pos + 1..].trim().to_string();
            headers.insert(key, value);
        }
    }

    let body_start = header_end + 4;
    Ok((status_code, headers, body_start))
}

fn extract_body_from_response(response: Vec<u8>) -> Vec<u8> {
    if let Some(pos) = response.windows(4).position(|window| window == b"\r\n\r\n") {
        response[pos + 4..].to_vec()
    } else {
        response
    }
}

fn path_with_query(url: &Url) -> String {
    match url.query() {
        Some(q) => format!("{}?{}", url.path(), q),
        None => url.path().to_string(),
    }
}

pub async fn send_http_request(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&[u8]>,
) -> RouterResult<Vec<u8>> {
    let parsed_url = Url::parse(url).map_err(|e| RouterError::Url(e.to_string()))?;
    let key = ConnectionKey::from_url(&parsed_url)?;
    let path_and_query = path_with_query(&parsed_url);

    debug!(
        "Forwarding {} {} to {}://{}:{}",
        method, path_and_query, key.scheme, key.host, key.port
    );

    let mut conn = CONNECTION_POOL.acquire(&key).await?;
    let request_bytes = build_request_bytes(method, &path_and_query, &key.host, headers, body);

    match send_request_on_connection(&mut conn, &request_bytes).await {
        Ok(response) => {
            CONNECTION_POOL.return_connection(&key, conn).await;
            Ok(extract_body_from_response(response))
        }
        Err(e) => {
            CONNECTION_POOL.recycle_connection(&key);
            Err(e)
        }
    }
}

async fn send_request_on_connection(
    conn: &mut PooledConnection,
    request_bytes: &[u8],
) -> RouterResult<Vec<u8>> {
    conn.write_all(request_bytes).await?;
    conn.flush().await?;

    let mut response = Vec::new();
    let mut buffer = [0; 4096];

    let mut headers_parsed = false;
    let mut content_length: Option<usize> = None;
    let mut body_start = 0;

    loop {
        let n = conn.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        response.extend_from_slice(&buffer[..n]);

        if !headers_parsed {
            if let Ok((_, headers, start)) = parse_http_response(&response) {
                headers_parsed = true;
                body_start = start;

                if let Some(cl) = headers.get("content-length") {
                    content_length = cl.parse().ok();
                }

                if let Some(expected_length) = content_length {
                    let body_received = response.len() - body_start;
                    if body_received >= expected_length {
                        break;
                    }
                }
            }
        } else if let Some(expected_length) = content_length {
            let body_received = response.len() - body_start;
            if body_received >= expected_length {
                break;
            }
        }
    }

    Ok(response)
}

pub async fn handle_streaming_request(
    client_stream: &mut TcpStream,
    url: &str,
    method: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &[u8],
    stream_config: Option<&StreamConfig>,
) -> RouterResult<()> {
    let buffer_size = stream_config.map(|c| c.buffer_size).unwrap_or(8192);
    let heartbeat_interval = stream_config
        .map(|c| Duration::from_secs(c.heartbeat_interval_secs))
        .unwrap_or(Duration::from_secs(30));

    let parsed_url = Url::parse(url).map_err(|e| RouterError::Url(e.to_string()))?;
    let key = ConnectionKey::from_url(&parsed_url)?;

    let request_bytes = build_request_bytes(method, path, &key.host, headers, Some(body));
    let mut conn = CONNECTION_POOL.acquire(&key).await?;

    match stream_response_to_client(
        &mut conn,
        client_stream,
        &request_bytes,
        buffer_size,
        heartbeat_interval,
    )
    .await
    {
        Ok(()) => {
            CONNECTION_POOL.return_connection(&key, conn).await;
            Ok(())
        }
        Err(e) => {
            CONNECTION_POOL.recycle_connection(&key);
            Err(e)
        }
    }
}

async fn stream_response_to_client(
    upstream_conn: &mut PooledConnection,
    client_stream: &mut TcpStream,
    request_bytes: &[u8],
    buffer_size: usize,
    heartbeat_interval: Duration,
) -> RouterResult<()> {
    upstream_conn.write_all(request_bytes).await?;
    upstream_conn.flush().await?;

    let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
    client_stream.write_all(response_headers.as_bytes()).await?;
    client_stream.flush().await?;

    stream_with_backpressure_and_heartbeat(
        upstream_conn,
        client_stream,
        buffer_size,
        heartbeat_interval,
    )
    .await
}

async fn stream_with_backpressure_and_heartbeat(
    upstream: &mut PooledConnection,
    client: &mut TcpStream,
    buffer_size: usize,
    heartbeat_interval: Duration,
) -> RouterResult<()> {
    let mut buffer = vec![0u8; buffer_size];
    let mut last_activity = Instant::now();
    let heartbeat_msg = b": heartbeat\n\n";

    loop {
        let timeout_duration = heartbeat_interval
            .checked_sub(last_activity.elapsed())
            .unwrap_or(Duration::from_millis(100));

        let read_result = smol::future::or(
            async {
                match upstream.read(&mut buffer).await {
                    Ok(n) => Some(Ok(n)),
                    Err(e) => Some(Err(e)),
                }
            },
            async {
                smol::Timer::after(timeout_duration).await;
                None
            },
        )
        .await;

        match read_result {
            Some(Ok(0)) => {
                debug!("Upstream closed connection, finishing stream");
                break;
            }
            Some(Ok(n)) => {
                if let Err(e) = client.write_all(&buffer[..n]).await {
                    if e.kind() == std::io::ErrorKind::BrokenPipe
                        || e.kind() == std::io::ErrorKind::ConnectionReset
                    {
                        warn!("Client disconnected during streaming, stopping gracefully");
                        return Ok(());
                    }
                    return Err(e.into());
                }

                if let Err(e) = client.flush().await {
                    if e.kind() == std::io::ErrorKind::BrokenPipe
                        || e.kind() == std::io::ErrorKind::ConnectionReset
                    {
                        warn!("Client disconnected during flush, stopping gracefully");
                        return Ok(());
                    }
                    return Err(e.into());
                }

                last_activity = Instant::now();
            }
            Some(Err(e)) => {
                if e.kind() == std::io::ErrorKind::ConnectionReset
                    || e.kind() == std::io::ErrorKind::BrokenPipe
                {
                    warn!("Upstream connection lost during streaming");
                    return Ok(());
                }
                return Err(e.into());
            }
            None => {
                if last_activity.elapsed() >= heartbeat_interval {
                    debug!("Sending heartbeat to keep connection alive");
                    if let Err(e) = client.write_all(heartbeat_msg).await {
                        if e.kind() == std::io::ErrorKind::BrokenPipe
                            || e.kind() == std::io::ErrorKind::ConnectionReset
                        {
                            warn!("Client disconnected while sending heartbeat");
                            return Ok(());
                        }
                        return Err(e.into());
                    }
                    if let Err(e) = client.flush().await {
                        if e.kind() == std::io::ErrorKind::BrokenPipe
                            || e.kind() == std::io::ErrorKind::ConnectionReset
                        {
                            warn!("Client disconnected during heartbeat flush");
                            return Ok(());
                        }
                        return Err(e.into());
                    }
                    last_activity = Instant::now();
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_bytes_creates_valid_http_request() {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert("Authorization".to_string(), "Bearer token123".to_string());

        let body = b"{\"key\":\"value\"}";
        let request = build_request_bytes("POST", "/api/test", "example.com", &headers, Some(body));

        let request_str = String::from_utf8_lossy(&request);
        assert!(request_str.contains("POST /api/test HTTP/1.1"));
        assert!(request_str.contains("Host: example.com"));
        assert!(request_str.contains("Connection: keep-alive"));
        assert!(request_str.contains("Content-Type: application/json"));
        assert!(request_str.contains("Authorization: Bearer token123"));
        assert!(request_str.contains("Content-Length: 15"));
        assert!(request_str.contains("{\"key\":\"value\"}"));
    }

    #[test]
    fn build_request_bytes_without_body() {
        let headers = HashMap::new();
        let request = build_request_bytes("GET", "/api/test", "example.com", &headers, None);

        let request_str = String::from_utf8_lossy(&request);
        assert!(request_str.contains("GET /api/test HTTP/1.1"));
        assert!(request_str.contains("Host: example.com"));
        assert!(!request_str.contains("Content-Length"));
        assert!(request_str.ends_with("\r\n"));
    }

    #[test]
    fn extract_body_from_response_splits_correctly() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nHello World";
        let body = extract_body_from_response(response.to_vec());
        assert_eq!(body, b"Hello World");
    }

    #[test]
    fn extract_body_from_response_returns_all_if_no_separator() {
        let response = b"Hello World";
        let body = extract_body_from_response(response.to_vec());
        assert_eq!(body, b"Hello World");
    }

    #[test]
    fn extract_body_from_response_handles_empty_body() {
        let response = b"HTTP/1.1 204 No Content\r\n\r\n";
        let body = extract_body_from_response(response.to_vec());
        assert_eq!(body, b"");
    }

    #[test]
    fn path_with_query_returns_path_only_when_no_query() {
        let url = Url::parse("https://example.com/api/test").unwrap();
        let result = path_with_query(&url);
        assert_eq!(result, "/api/test");
    }

    #[test]
    fn path_with_query_includes_query_string() {
        let url = Url::parse("https://example.com/api/test?key=value&foo=bar").unwrap();
        let result = path_with_query(&url);
        assert_eq!(result, "/api/test?key=value&foo=bar");
    }

    #[test]
    fn path_with_query_handles_root_path() {
        let url = Url::parse("https://example.com").unwrap();
        let result = path_with_query(&url);
        assert_eq!(result, "/");
    }

    #[test]
    fn path_with_query_handles_empty_path_with_query() {
        let url = Url::parse("https://example.com?query=test").unwrap();
        let result = path_with_query(&url);
        assert_eq!(result, "/?query=test");
    }

    #[test]
    fn stream_config_default_buffer_size() {
        let config = StreamConfig {
            buffer_size: 16384,
            heartbeat_interval_secs: 60,
        };
        assert_eq!(config.buffer_size, 16384);
    }

    #[test]
    fn stream_config_default_heartbeat() {
        let config = StreamConfig {
            buffer_size: 8192,
            heartbeat_interval_secs: 45,
        };
        assert_eq!(config.heartbeat_interval_secs, 45);
    }

    #[test]
    fn connection_key_from_url_https() {
        let url = Url::parse("https://api.example.com/v1/chat").unwrap();
        let key = ConnectionKey::from_url(&url).unwrap();
        assert_eq!(key.scheme, "https");
        assert_eq!(key.host, "api.example.com");
        assert_eq!(key.port, 443);
    }

    #[test]
    fn connection_key_from_url_http() {
        let url = Url::parse("http://api.example.com:8080/v1/chat").unwrap();
        let key = ConnectionKey::from_url(&url).unwrap();
        assert_eq!(key.scheme, "http");
        assert_eq!(key.host, "api.example.com");
        assert_eq!(key.port, 8080);
    }

    #[test]
    fn pool_config_default_values() {
        let config = PoolConfig::default();
        assert_eq!(config.max_size, DEFAULT_POOL_MAX_SIZE);
        assert_eq!(
            config.idle_timeout,
            Duration::from_secs(DEFAULT_POOL_IDLE_TIMEOUT_SECS)
        );
    }

    #[test]
    fn parse_http_response_valid() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 13\r\n\r\n{\"ok\":true}";
        let (status, headers, body_start) = parse_http_response(response).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
        assert_eq!(headers.get("content-length").unwrap(), "13");
        assert_eq!(body_start, 71);
    }

    #[test]
    fn parse_http_response_invalid_no_separator() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Type: application/json";
        let result = parse_http_response(response);
        assert!(result.is_err());
    }
}
