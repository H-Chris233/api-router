use std::collections::HashMap;
use crate::errors::{RouterResult, RouterError};
use futures_lite::AsyncReadExt;
use futures_lite::AsyncWriteExt;
use log::debug;
use smol::net::TcpStream;
use url::Url;
use async_tls::TlsConnector;

// 发送HTTP/HTTPS请求的辅助函数
pub async fn send_http_request(
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<&str>,
) -> RouterResult<String> {
    let parsed_url = url::Url::parse(url).map_err(|e| RouterError::Url(e.to_string()))?;
    let host = parsed_url
        .host_str()
        .ok_or_else(|| RouterError::Url("Invalid URL: missing host".to_string()))?;
    let port = parsed_url
        .port_or_known_default()
        .unwrap_or(if parsed_url.scheme() == "https" { 443 } else { 80 });
    let path_and_query = parsed_url.path();

    debug!("Forwarding {} {} to {}:{}", method, path_and_query, host, port);

    let mut tcp_stream = TcpStream::connect((host, port)).await?;

    // 构建请求
    let mut request = String::new();
    request.push_str(&format!("{} {} HTTP/1.1\r\n", method, path_and_query));
    request.push_str(&format!("Host: {}\r\n", host));
    request.push_str("Connection: close\r\n");

    for (key, value) in headers {
        request.push_str(&format!("{}: {}\r\n", key, value));
    }

    if let Some(body_str) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body_str.len()));
        request.push_str("\r\n");
        request.push_str(body_str);
    } else {
        request.push_str("\r\n");
    }

    // 根据协议选择不同的处理方式
    if parsed_url.scheme() == "https" {
        let tls_connector = TlsConnector::new();
        let mut tls_stream = tls_connector
            .connect(host, tcp_stream)
            .await
            .map_err(|e| RouterError::Tls(e.to_string()))?;
        tls_stream.write_all(request.as_bytes()).await?;
        tls_stream.flush().await?;

        // 读取响应
        let mut response = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = tls_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..n]);
        }

        // 解析响应，找到主体部分
        let response_str = String::from_utf8_lossy(&response);
        if let Some(body_start) = response_str.find("\r\n\r\n") {
            Ok(response_str[body_start + 4..].to_string())
        } else {
            Ok(response_str.to_string())
        }
    } else {
        // HTTP请求处理
        tcp_stream.write_all(request.as_bytes()).await?;
        tcp_stream.flush().await?;

        // 读取响应
        let mut response = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            let n = tcp_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..n]);
        }

        // 解析响应，找到主体部分
        let response_str = String::from_utf8_lossy(&response);
        if let Some(body_start) = response_str.find("\r\n\r\n") {
            Ok(response_str[body_start + 4..].to_string())
        } else {
            Ok(response_str.to_string())
        }
    }
}

// 处理SSE流式响应的函数
pub async fn handle_streaming_request(
    client_stream: &mut TcpStream,
    url: &str,
    path: &str,
    headers: &HashMap<String, String>,
    body: &str,
) -> RouterResult<()> {
    let parsed_url = url::Url::parse(url).map_err(|e| RouterError::Url(e.to_string()))?;
    let host = parsed_url
        .host_str()
        .ok_or_else(|| RouterError::Url("Invalid URL: missing host".to_string()))?;
    let port = parsed_url
        .port_or_known_default()
        .unwrap_or(if parsed_url.scheme() == "https" { 443 } else { 80 });

    let mut tcp_stream = TcpStream::connect((host, port)).await?;

    // 根据协议选择不同的处理方式
    if parsed_url.scheme() == "https" {
        let tls_connector = TlsConnector::new();
        let mut tls_stream = tls_connector
            .connect(host, tcp_stream)
            .await
            .map_err(|e| RouterError::Tls(e.to_string()))?;

        // 构建流式请求
        let mut request = String::new();
        request.push_str(&format!("POST {} HTTP/1.1\r\n", path));
        request.push_str(&format!("Host: {}\r\n", host));
        request.push_str("Connection: close\r\n");

        // 添加自定义头部
        for (key, value) in headers {
            request.push_str(&format!("{}: {}\r\n", key, value));
        }

        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        request.push_str("\r\n");
        request.push_str(body);

        tls_stream.write_all(request.as_bytes()).await?;
        tls_stream.flush().await?;

        // 发送SSE响应头给客户端
        let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
        client_stream.write_all(response_headers.as_bytes()).await?;

        // 转发API响应到客户端
        let mut buffer = [0; 4096];
        loop {
            let n = tls_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            // 将数据写回客户端
            client_stream.write_all(&buffer[..n]).await?;
            client_stream.flush().await?;
        }
    } else {
        // HTTP请求处理
        // 构建流式请求
        let mut request = String::new();
        request.push_str(&format!("POST {} HTTP/1.1\r\n", path));
        request.push_str(&format!("Host: {}\r\n", host));
        request.push_str("Connection: close\r\n");

        // 添加自定义头部
        for (key, value) in headers {
            request.push_str(&format!("{}: {}\r\n", key, value));
        }

        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        request.push_str("\r\n");
        request.push_str(body);

        tcp_stream.write_all(request.as_bytes()).await?;
        tcp_stream.flush().await?;

        // 发送SSE响应头给客户端
        let response_headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
        client_stream.write_all(response_headers.as_bytes()).await?;

        // 转发API响应到客户端
        let mut buffer = [0; 4096];
        loop {
            let n = tcp_stream.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            // 将数据写回客户端
            client_stream.write_all(&buffer[..n]).await?;
            client_stream.flush().await?;
        }
    }

    Ok(())
}