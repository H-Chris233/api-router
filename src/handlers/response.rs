use crate::errors::{RouterError, RouterResult};
use smol::io::AsyncWriteExt;
use smol::net::TcpStream;
use std::io::Write as IoWrite;

pub(super) fn build_error_response(status_code: u16, reason: &str, message: &str) -> Vec<u8> {
    build_error_response_with_headers(status_code, reason, message, &[])
}

pub(super) fn build_error_response_with_headers(
    status_code: u16,
    reason: &str,
    message: &str,
    extra_headers: &[(&str, String)],
) -> Vec<u8> {
    let body = serde_json::to_vec(&serde_json::json!({
        "error": {
            "message": message,
        }
    }))
    .expect("JSON serialization should not fail");

    let mut response = Vec::with_capacity(128 + body.len());
    write!(
        &mut response,
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        status_code,
        reason,
        body.len()
    )
    .expect("writing to Vec<u8> cannot fail");

    for (key, value) in extra_headers {
        write!(&mut response, "{}: {}\r\n", key, value).expect("writing to Vec<u8> cannot fail");
    }

    response.extend_from_slice(b"\r\n");
    response.extend_from_slice(&body);
    response
}

pub(super) fn map_error_to_response(err: &RouterError) -> Vec<u8> {
    match err {
        RouterError::BadRequest(msg) => build_error_response(400, "BAD REQUEST", msg),
        RouterError::ConfigRead(msg) | RouterError::ConfigParse(msg) => {
            build_error_response(500, "INTERNAL SERVER ERROR", msg)
        }
        RouterError::Url(msg) | RouterError::Tls(msg) | RouterError::Upstream(msg) => {
            build_error_response(502, "BAD GATEWAY", msg)
        }
        RouterError::Io(msg) => {
            build_error_response(500, "INTERNAL SERVER ERROR", &msg.to_string())
        }
        RouterError::Json(msg) => build_error_response(400, "BAD REQUEST", &msg.to_string()),
    }
}

pub(super) async fn write_success(
    stream: &mut TcpStream,
    content_type: &str,
    payload: &[u8],
) -> RouterResult<()> {
    let mut response = Vec::with_capacity(128 + payload.len());
    write!(
        &mut response,
        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n",
        content_type,
        payload.len()
    )
    .expect("writing to Vec<u8> cannot fail");
    response.extend_from_slice(payload);
    stream.write_all(&response).await?;
    stream.flush().await?;
    Ok(())
}
