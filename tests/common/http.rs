use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn body_utf8(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(|s| s.as_str())
    }
}

pub fn send_http_request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&[u8]>,
) -> HttpResponse {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("failed to connect to router");
    stream
        .set_nodelay(true)
        .expect("failed to disable Nagle algorithm");

    let mut request = format!(
        "{} {} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n",
        method, path, port
    );

    let mut has_content_length = false;
    for (name, value) in headers {
        if name.eq_ignore_ascii_case("content-length") {
            has_content_length = true;
        }
        request.push_str(&format!("{}: {}\r\n", name, value));
    }

    let body_bytes = body.unwrap_or(&[]);
    if !has_content_length {
        request.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    request.push_str("\r\n");

    stream
        .write_all(request.as_bytes())
        .expect("failed to send request headers");
    if !body_bytes.is_empty() {
        stream
            .write_all(body_bytes)
            .expect("failed to send request body");
    }
    stream.flush().expect("failed to flush request");

    let mut raw_response = Vec::new();
    stream
        .read_to_end(&mut raw_response)
        .expect("failed to read response");

    parse_http_response(&raw_response)
}

fn parse_http_response(raw: &[u8]) -> HttpResponse {
    let header_end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("invalid HTTP response: missing header terminator");
    let (header_bytes, body_bytes) = raw.split_at(header_end + 4);
    let header_str = String::from_utf8_lossy(header_bytes);
    let mut lines = header_str.split("\r\n").filter(|line| !line.is_empty());
    let status_line = lines
        .next()
        .expect("invalid HTTP response: missing status line");
    let mut status_parts = status_line.split_whitespace();
    let _http_version = status_parts.next().unwrap_or("HTTP/1.1");
    let status_code: u16 = status_parts
        .next()
        .unwrap_or("200")
        .parse()
        .expect("invalid status code");

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    HttpResponse {
        status: status_code,
        headers,
        body: body_bytes.to_vec(),
    }
}
