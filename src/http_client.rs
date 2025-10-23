use crate::errors::{RouterError, RouterResult};
use async_tls::TlsConnector;
use smol::io::{AsyncReadExt, AsyncWriteExt};
use log::debug;
use smol::net::TcpStream;
use std::collections::HashMap;
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
) -> RouterResult<()> {
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
    let mut tcp_stream = TcpStream::connect((host, port)).await?;

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

        let mut buffer = [0; 4096];
        loop {
            let n = tls_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            client_stream.write_all(&buffer[..n]).await?;
            client_stream.flush().await?;
        }
    } else {
        tcp_stream.write_all(&request_bytes).await?;
        tcp_stream.flush().await?;

        let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
        client_stream.write_all(response_headers.as_bytes()).await?;

        let mut buffer = [0; 4096];
        loop {
            let n = tcp_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            client_stream.write_all(&buffer[..n]).await?;
            client_stream.flush().await?;
        }
    }

    Ok(())
}
