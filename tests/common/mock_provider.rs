use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use super::pick_free_port;

#[derive(Debug, Clone)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub data: Vec<u8>,
    pub delay: Option<Duration>,
}

impl StreamChunk {
    pub fn new(data: impl Into<Vec<u8>>) -> Self {
        Self {
            data: data.into(),
            delay: None,
        }
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }
}

#[derive(Debug, Clone)]
pub enum MockBody {
    Static(Vec<u8>),
    Stream { chunks: Vec<StreamChunk> },
}

#[derive(Debug, Clone)]
pub struct MockResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: MockBody,
}

impl MockResponse {
    pub fn json(status: u16, value: Value) -> Self {
        let body = serde_json::to_vec(&value).expect("failed to serialize mock response");
        Self {
            status,
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: MockBody::Static(body),
        }
    }

    pub fn bytes(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: MockBody::Static(body.into()),
        }
    }

    pub fn stream<I, K, V>(status: u16, headers: I, chunks: Vec<StreamChunk>) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let headers = headers
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        Self {
            status,
            headers,
            body: MockBody::Stream { chunks },
        }
    }
}

pub struct MockProvider {
    port: u16,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    _responses: Arc<Mutex<HashMap<String, MockResponse>>>,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockProvider {
    pub fn builder() -> MockProviderBuilder {
        MockProviderBuilder::default()
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn received_requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }

    fn start(responses: HashMap<String, MockResponse>) -> Self {
        let port = pick_free_port();
        let listener =
            TcpListener::bind(("127.0.0.1", port)).expect("failed to bind mock provider port");
        listener
            .set_nonblocking(true)
            .expect("failed to configure listener");

        let requests = Arc::new(Mutex::new(Vec::new()));
        let responses = Arc::new(Mutex::new(responses));
        let shutdown = Arc::new(AtomicBool::new(false));

        let requests_clone = Arc::clone(&requests);
        let responses_clone = Arc::clone(&responses);
        let shutdown_clone = Arc::clone(&shutdown);

        let thread = thread::spawn(move || {
            run_server(listener, requests_clone, responses_clone, shutdown_clone)
        });

        Self {
            port,
            requests,
            _responses: responses,
            shutdown,
            thread: Some(thread),
        }
    }
}

impl Drop for MockProvider {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Default)]
pub struct MockProviderBuilder {
    routes: HashMap<String, MockResponse>,
}

impl MockProviderBuilder {
    pub fn route(mut self, path: &str, response: MockResponse) -> Self {
        self.routes.insert(path.to_string(), response);
        self
    }

    pub fn build(self) -> MockProvider {
        MockProvider::start(self.routes)
    }
}

fn run_server(
    listener: TcpListener,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    responses: Arc<Mutex<HashMap<String, MockResponse>>>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Some(request) = read_request(&mut stream) {
                    requests.lock().unwrap().push(request.clone());
                    let response = resolve_response(&request, &responses);
                    if let Some(response) = response {
                        let _ = send_response(&mut stream, response);
                    } else {
                        let not_found = MockResponse::bytes(404, b"Not Found".to_vec());
                        let _ = send_response(&mut stream, not_found);
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => {
                eprintln!("mock provider accept error: {}", err);
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<RecordedRequest> {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    let mut header_len = None;
    let mut expected_len = None;

    loop {
        match stream.read(&mut temp) {
            Ok(0) => break,
            Ok(n) => {
                buffer.extend_from_slice(&temp[..n]);
                if header_len.is_none() {
                    if let Some(pos) = find_header_end(&buffer) {
                        header_len = Some(pos + 4);
                        if let Some(len) = parse_content_length(&buffer[..pos]) {
                            expected_len = Some(pos + 4 + len);
                        }
                    }
                }
                if let Some(len) = expected_len {
                    if buffer.len() >= len {
                        break;
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(1));
            }
            Err(_) => return None,
        }
    }

    parse_recorded_request(&buffer)
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_content_length(headers: &[u8]) -> Option<usize> {
    let header_str = String::from_utf8_lossy(headers);
    for line in header_str.split("\r\n") {
        if let Some(value) = line.strip_prefix("Content-Length:") {
            return value.trim().parse().ok();
        }
        if let Some(value) = line.strip_prefix("content-length:") {
            return value.trim().parse().ok();
        }
    }
    None
}

fn parse_recorded_request(buffer: &[u8]) -> Option<RecordedRequest> {
    let header_end = find_header_end(buffer)?;
    let header_bytes = &buffer[..header_end];
    let body = buffer[header_end + 4..].to_vec();
    let header_str = String::from_utf8_lossy(header_bytes);
    let mut lines = header_str.split("\r\n").filter(|line| !line.is_empty());
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    Some(RecordedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn resolve_response(
    request: &RecordedRequest,
    responses: &Arc<Mutex<HashMap<String, MockResponse>>>,
) -> Option<MockResponse> {
    let map = responses.lock().unwrap();
    if let Some(response) = map.get(&request.path) {
        return Some(response.clone());
    }
    if let Some((path, _query)) = request.path.split_once('?') {
        if let Some(response) = map.get(path) {
            return Some(response.clone());
        }
    }
    None
}

fn send_response(stream: &mut TcpStream, response: MockResponse) -> std::io::Result<()> {
    match response.body {
        MockBody::Static(body) => {
            let mut response_text = format!(
                "HTTP/1.1 {} {}\r\n",
                response.status,
                reason_phrase(response.status)
            );
            let mut has_content_length = false;
            for (name, value) in &response.headers {
                if name.eq_ignore_ascii_case("content-length") {
                    has_content_length = true;
                }
                response_text.push_str(&format!("{}: {}\r\n", name, value));
            }
            if !has_content_length {
                response_text.push_str(&format!("Content-Length: {}\r\n", body.len()));
            }
            response_text.push_str("Connection: close\r\n\r\n");
            stream.write_all(response_text.as_bytes())?;
            stream.write_all(&body)?;
            stream.flush()?;
        }
        MockBody::Stream { chunks } => {
            let mut response_text = format!(
                "HTTP/1.1 {} {}\r\n",
                response.status,
                reason_phrase(response.status)
            );
            let mut has_connection_close = false;
            let mut has_content_type = false;
            for (name, value) in &response.headers {
                if name.eq_ignore_ascii_case("connection") {
                    has_connection_close = true;
                }
                if name.eq_ignore_ascii_case("content-type") {
                    has_content_type = true;
                }
                response_text.push_str(&format!("{}: {}\r\n", name, value));
            }
            if !has_content_type {
                response_text.push_str("Content-Type: text/event-stream\r\n");
            }
            if !has_connection_close {
                response_text.push_str("Connection: close\r\n");
            }
            response_text.push_str("Cache-Control: no-cache\r\n\r\n");
            stream.write_all(response_text.as_bytes())?;
            stream.flush()?;

            for chunk in &chunks {
                stream.write_all(&chunk.data)?;
                stream.flush()?;
                if let Some(delay) = chunk.delay {
                    thread::sleep(delay);
                }
            }
        }
    }
    Ok(())
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "CREATED",
        204 => "NO CONTENT",
        400 => "BAD REQUEST",
        401 => "UNAUTHORIZED",
        403 => "FORBIDDEN",
        404 => "NOT FOUND",
        429 => "TOO MANY REQUESTS",
        500 => "INTERNAL SERVER ERROR",
        502 => "BAD GATEWAY",
        _ => "OK",
    }
}
