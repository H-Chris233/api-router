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

    #[test]
    fn url_error_displays_correctly() {
        let error = RouterError::Url("invalid url".to_string());
        assert_eq!(format!("{}", error), "URL error: invalid url");
    }

    #[test]
    fn config_read_error_displays_correctly() {
        let error = RouterError::ConfigRead("file not found".to_string());
        assert_eq!(format!("{}", error), "Config read error: file not found");
    }

    #[test]
    fn config_parse_error_displays_correctly() {
        let error = RouterError::ConfigParse("invalid json".to_string());
        assert_eq!(format!("{}", error), "Config parse error: invalid json");
    }

    #[test]
    fn upstream_error_displays_correctly() {
        let error = RouterError::Upstream("500 Internal Server Error".to_string());
        assert_eq!(format!("{}", error), "Upstream error: 500 Internal Server Error");
    }

    #[test]
    fn tls_error_displays_correctly() {
        let error = RouterError::Tls("certificate validation failed".to_string());
        assert_eq!(format!("{}", error), "TLS error: certificate validation failed");
    }

    #[test]
    fn bad_request_error_displays_correctly() {
        let error = RouterError::BadRequest("missing required field".to_string());
        assert_eq!(format!("{}", error), "Bad request: missing required field");
    }

    #[test]
    fn io_error_conversion_works() {
        let io_error = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let router_error: RouterError = io_error.into();
        assert!(matches!(router_error, RouterError::Io(_)));
        assert!(format!("{}", router_error).contains("file not found"));
    }

    #[test]
    fn json_error_conversion_works() {
        let json_error = serde_json::from_str::<serde_json::Value>("{invalid}").unwrap_err();
        let router_error: RouterError = json_error.into();
        assert!(matches!(router_error, RouterError::Json(_)));
    }

    #[test]
    fn router_result_ok_works() {
        let result: RouterResult<i32> = Ok(42);
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn router_result_err_works() {
        let result: RouterResult<i32> = Err(RouterError::Url("bad url".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn error_debug_format_works() {
        let error = RouterError::BadRequest("test".to_string());
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("BadRequest"));
        assert!(debug_str.contains("test"));
    }
}
