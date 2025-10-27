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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    #[test]
    fn url_error_displays_correctly() {
        let error = RouterError::Url("invalid scheme".to_string());
        assert_eq!(error.to_string(), "URL error: invalid scheme");
    }

    #[test]
    fn io_error_converts_from_std_io_error() {
        let io_error = io::Error::new(io::ErrorKind::ConnectionRefused, "connection refused");
        let router_error: RouterError = io_error.into();
        assert!(matches!(router_error, RouterError::Io(_)));
        assert!(router_error.to_string().contains("connection refused"));
    }

    #[test]
    fn config_read_error_displays_correctly() {
        let error = RouterError::ConfigRead("file not found".to_string());
        assert_eq!(error.to_string(), "Config read error: file not found");
    }

    #[test]
    fn config_parse_error_displays_correctly() {
        let error = RouterError::ConfigParse("invalid json".to_string());
        assert_eq!(error.to_string(), "Config parse error: invalid json");
    }

    #[test]
    fn json_error_converts_from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("{invalid").unwrap_err();
        let router_error: RouterError = json_err.into();
        assert!(matches!(router_error, RouterError::Json(_)));
        assert!(router_error.to_string().contains("JSON error"));
    }

    #[test]
    fn upstream_error_displays_correctly() {
        let error = RouterError::Upstream("502 Bad Gateway".to_string());
        assert_eq!(error.to_string(), "Upstream error: 502 Bad Gateway");
    }

    #[test]
    fn tls_error_displays_correctly() {
        let error = RouterError::Tls("certificate verification failed".to_string());
        assert_eq!(
            error.to_string(),
            "TLS error: certificate verification failed"
        );
    }

    #[test]
    fn bad_request_error_displays_correctly() {
        let error = RouterError::BadRequest("missing model field".to_string());
        assert_eq!(error.to_string(), "Bad request: missing model field");
    }

    #[test]
    fn router_result_ok_works() {
        let result: RouterResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn router_result_err_works() {
        let result: RouterResult<i32> = Err(RouterError::BadRequest("test".to_string()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test"));
    }

    #[test]
    fn error_is_send_and_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<RouterError>();
        assert_sync::<RouterError>();
    }
}
