//! 轻量级 URL 解析器
//!
//! 只支持 HTTP/HTTPS URL，无需国际化域名支持

use crate::errors::{RouterError, RouterResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    scheme: String,
    host: String,
    port: Option<u16>,
    path: String,
    query: Option<String>,
}

impl Url {
    pub fn parse(url: &str) -> RouterResult<Self> {
        let url = url.trim();

        // 解析 scheme
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| RouterError::Url("Missing scheme".to_string()))?;

        if scheme != "http" && scheme != "https" {
            return Err(RouterError::Url(format!("Unsupported scheme: {}", scheme)));
        }

        // 分离 host:port 和 path?query
        let (host_port, path_query) = if let Some((hp, pq)) = rest.split_once('/') {
            (hp, format!("/{}", pq))
        } else if let Some((hp, q)) = rest.split_once('?') {
            (hp, format!("/?{}", q))
        } else {
            (rest, "/".to_string())
        };

        // 解析 host 和 port
        let (host, port) = match host_port.split_once(':') {
            Some((h, p)) => {
                let port = p
                    .parse::<u16>()
                    .map_err(|_| RouterError::Url(format!("Invalid port: {}", p)))?;
                (h.to_string(), Some(port))
            }
            None => (host_port.to_string(), None),
        };

        if host.is_empty() {
            return Err(RouterError::Url("Missing host".to_string()));
        }

        // 分离 path 和 query
        let (path, query) = match path_query.split_once('?') {
            Some((p, q)) => (p.to_string(), Some(q.to_string())),
            None => (path_query, None),
        };

        Ok(Url {
            scheme: scheme.to_string(),
            host,
            port,
            path,
            query,
        })
    }

    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    pub fn host_str(&self) -> Option<&str> {
        Some(&self.host)
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    pub fn port_or_known_default(&self) -> Option<u16> {
        Some(
            self.port
                .unwrap_or(if self.scheme == "https" { 443 } else { 80 }),
        )
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_https_url() {
        let url = Url::parse("https://example.com/api/test").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("example.com"));
        assert_eq!(url.port_or_known_default(), Some(443));
        assert_eq!(url.path(), "/api/test");
        assert_eq!(url.query(), None);
    }

    #[test]
    fn parse_http_url_with_port() {
        let url = Url::parse("http://api.example.com:8080/v1/chat").unwrap();
        assert_eq!(url.scheme(), "http");
        assert_eq!(url.host_str(), Some("api.example.com"));
        assert_eq!(url.port(), Some(8080));
        assert_eq!(url.path(), "/v1/chat");
    }

    #[test]
    fn parse_url_with_query() {
        let url = Url::parse("https://example.com/api/test?key=value&foo=bar").unwrap();
        assert_eq!(url.path(), "/api/test");
        assert_eq!(url.query(), Some("key=value&foo=bar"));
    }

    #[test]
    fn parse_url_root_path() {
        let url = Url::parse("https://example.com").unwrap();
        assert_eq!(url.path(), "/");
    }

    #[test]
    fn parse_url_query_only() {
        let url = Url::parse("https://example.com?query=test").unwrap();
        assert_eq!(url.path(), "/");
        assert_eq!(url.query(), Some("query=test"));
    }

    #[test]
    fn parse_invalid_scheme() {
        assert!(Url::parse("ftp://example.com").is_err());
    }

    #[test]
    fn parse_missing_scheme() {
        assert!(Url::parse("example.com").is_err());
    }

    #[test]
    fn parse_invalid_port() {
        assert!(Url::parse("http://example.com:abc/path").is_err());
    }
}
