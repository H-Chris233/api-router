use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

fn start_test_server(port: u16) -> Child {
    Command::new("cargo")
        .args(&["run", "--", "qwen", &port.to_string()])
        .spawn()
        .expect("failed to start server")
}

fn wait_for_server(port: u16, max_attempts: u32) {
    for _ in 0..max_attempts {
        if TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("server did not start in time");
}

fn send_request(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .expect("failed to connect to server");

    stream
        .write_all(request.as_bytes())
        .expect("failed to write request");

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    let mut content_length = 0;
    let mut in_headers = true;

    for line in reader.by_ref().lines() {
        let line = line.expect("failed to read line");
        if in_headers {
            if line.is_empty() {
                in_headers = false;
                continue;
            }
            if line.to_lowercase().starts_with("content-length:") {
                content_length = line
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0);
            }
            response.push_str(&line);
            response.push('\n');
        } else {
            response.push_str(&line);
            response.push('\n');
            if response.len() - response.lines().take_while(|l| !l.is_empty()).count() >= content_length {
                break;
            }
        }
    }

    response
}

#[test]
#[ignore]
fn metrics_endpoint_returns_prometheus_format() {
    let port = 8123;
    let mut server = start_test_server(port);
    wait_for_server(port, 50);

    let health_request = format!(
        "GET /health HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        port
    );
    let _health_response = send_request(port, &health_request);

    let metrics_request = format!(
        "GET /metrics HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        port
    );
    let response = send_request(port, &metrics_request);

    assert!(response.contains("HTTP/1.1 200 OK"));
    assert!(response.contains("Content-Type: text/plain"));

    assert!(response.contains("requests_total"));
    assert!(response.contains("request_latency_seconds"));
    assert!(response.contains("active_connections"));
    assert!(response.contains("rate_limiter_buckets"));

    assert!(response.contains("route=\"/health\""));
    assert!(response.contains("method=\"GET\""));
    assert!(response.contains("status=\"200\""));

    server.kill().expect("failed to kill server");
}

#[test]
#[ignore]
fn metrics_tracks_request_counts() {
    let port = 8124;
    let mut server = start_test_server(port);
    wait_for_server(port, 50);

    let health_request = format!(
        "GET /health HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        port
    );
    
    for _ in 0..5 {
        let _response = send_request(port, &health_request);
        thread::sleep(Duration::from_millis(50));
    }

    let metrics_request = format!(
        "GET /metrics HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        port
    );
    let response = send_request(port, &metrics_request);

    assert!(response.contains("requests_total"));
    assert!(response.contains("route=\"/health\""));

    server.kill().expect("failed to kill server");
}

#[test]
#[ignore]
fn metrics_tracks_latency() {
    let port = 8125;
    let mut server = start_test_server(port);
    wait_for_server(port, 50);

    let health_request = format!(
        "GET /health HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        port
    );
    let _response = send_request(port, &health_request);

    let metrics_request = format!(
        "GET /metrics HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        port
    );
    let response = send_request(port, &metrics_request);

    assert!(response.contains("request_latency_seconds_bucket"));
    assert!(response.contains("request_latency_seconds_sum"));
    assert!(response.contains("request_latency_seconds_count"));

    server.kill().expect("failed to kill server");
}

#[test]
#[ignore]
fn metrics_tracks_404_errors() {
    let port = 8126;
    let mut server = start_test_server(port);
    wait_for_server(port, 50);

    let not_found_request = format!(
        "GET /nonexistent HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        port
    );
    let _response = send_request(port, &not_found_request);

    let metrics_request = format!(
        "GET /metrics HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        port
    );
    let response = send_request(port, &metrics_request);

    assert!(response.contains("requests_total"));
    assert!(response.contains("status=\"404\""));

    server.kill().expect("failed to kill server");
}
