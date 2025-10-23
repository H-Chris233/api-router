use crate::errors::{RouterError, RouterResult};
use crate::config::StreamConfig;
use async_tls::TlsConnector;
use smol::io::{AsyncReadExt, AsyncWriteExt};
use log::{debug, warn};
use smol::net::TcpStream;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use url::Url;

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
    request.extend_from_slice(b"Connection: close\r\n");

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

fn extract_body_from_response(response: Vec<u8>) -> Vec<u8> {
    if let Some(pos) = response.windows(4).position(|window| window == b"\r\n\r\n") {
        response[pos + 4..].to_vec()
    } else {
        response
    }
}

fn path_with_query(url: &Url) -> String {
    let mut combined = url.path().to_string();
    if let Some(query) = url.query() {
        combined.push('?');
        combined.push_str(query);
    }
    if combined.is_empty() {
        combined.push('/');
    }
    combined
}

// 发送HTTP/HTTPS请求的辅助函数
pub async fn send_http_request(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&[u8]>,
) -> RouterResult<Vec<u8>> {
    let parsed_url = Url::parse(url).map_err(|e| RouterError::Url(e.to_string()))?;
    let host = parsed_url
        .host_str()
        .ok_or_else(|| RouterError::Url("Invalid URL: missing host".to_string()))?;
    let port = parsed_url
        .port_or_known_default()
        .unwrap_or(if parsed_url.scheme() == "https" {
            443
        } else {
            80
        });
    let path_and_query = path_with_query(&parsed_url);

    debug!(
        "Forwarding {} {} to {}:{}",
        method, path_and_query, host, port
    );

    let mut tcp_stream = TcpStream::connect((host, port)).await?;
    let request_bytes = build_request_bytes(method, &path_and_query, host, headers, body);

    if parsed_url.scheme() == "https" {
        let tls_connector = TlsConnector::new();
        let mut tls_stream = tls_connector
            .connect(host, tcp_stream)
            .await
            .map_err(|e| RouterError::Tls(e.to_string()))?;
        tls_stream.write_all(&request_bytes).await?;
        tls_stream.flush().await?;

        let mut response = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = tls_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..n]);
        }

        Ok(extract_body_from_response(response))
    } else {
        tcp_stream.write_all(&request_bytes).await?;
        tcp_stream.flush().await?;

        let mut response = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = tcp_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..n]);
        }

        Ok(extract_body_from_response(response))
    }
}

// 处理SSE流式响应的函数
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
    let host = parsed_url
        .host_str()
        .ok_or_else(|| RouterError::Url("Invalid URL: missing host".to_string()))?;
    let port = parsed_url
        .port_or_known_default()
        .unwrap_or(if parsed_url.scheme() == "https" {
            443
        } else {
            80
        });

    let request_bytes = build_request_bytes(method, path, host, headers, Some(body));
    let tcp_stream = TcpStream::connect((host, port)).await?;

    if parsed_url.scheme() == "https" {
        let tls_connector = TlsConnector::new();
        let mut tls_stream = tls_connector
            .connect(host, tcp_stream)
            .await
            .map_err(|e| RouterError::Tls(e.to_string()))?;

        tls_stream.write_all(&request_bytes).await?;
        tls_stream.flush().await?;

        let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
        client_stream.write_all(response_headers.as_bytes()).await?;
        client_stream.flush().await?;

        stream_with_backpressure_and_heartbeat(
            &mut tls_stream,
            client_stream,
            buffer_size,
            heartbeat_interval,
        )
        .await
    } else {
        let mut tcp_stream = tcp_stream;
        tcp_stream.write_all(&request_bytes).await?;
        tcp_stream.flush().await?;

        let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
        client_stream.write_all(response_headers.as_bytes()).await?;
        client_stream.flush().await?;

        stream_with_backpressure_and_heartbeat(
            &mut tcp_stream,
            client_stream,
            buffer_size,
            heartbeat_interval,
        )
        .await
    }
}

async fn stream_with_backpressure_and_heartbeat<R, W>(
    upstream: &mut R,
    client: &mut W,
    buffer_size: usize,
    heartbeat_interval: Duration,
) -> RouterResult<()>
where
    R: AsyncReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
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
