use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RouterError {
    #[error("URL error: {0}")]
    Url(String),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Config read error: {0}")]
    ConfigRead(String),
    #[error("Config parse error: {0}")]
    ConfigParse(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Upstream error: {0}")]
    Upstream(String),
    #[error("TLS error: {0}")]
    Tls(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
}

pub type RouterResult<T> = Result<T, RouterError>;
