use crate::errors::RouterError;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use sentry::protocol::{Event, Level};
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

const UPSTREAM_FAILURE_THRESHOLD: u64 = 5;
const UPSTREAM_FAILURE_WINDOW_SECS: u64 = 300; // 5 minutes

static UPSTREAM_FAILURE_TRACKER: Lazy<DashMap<String, UpstreamFailureInfo>> =
    Lazy::new(DashMap::new);

#[derive(Debug)]
struct UpstreamFailureInfo {
    count: AtomicU64,
    first_failure: Instant,
    last_alerted: Option<Instant>,
}

pub struct SentryConfig {
    pub dsn: Option<String>,
    pub sample_rate: f32,
    pub environment: String,
    pub enabled: bool,
}

impl SentryConfig {
    pub fn from_env() -> Self {
        let dsn = env::var("SENTRY_DSN").ok();
        let enabled = dsn.is_some();

        let sample_rate = env::var("SENTRY_SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        let environment = env::var("SENTRY_ENVIRONMENT")
            .unwrap_or_else(|_| "production".to_string());

        SentryConfig {
            dsn,
            sample_rate,
            environment,
            enabled,
        }
    }
}

pub fn init_sentry(config: &SentryConfig) -> Option<sentry::ClientInitGuard> {
    if !config.enabled {
        info!("Sentry error tracking is disabled (no SENTRY_DSN configured)");
        return None;
    }

    let dsn = config.dsn.as_ref()?;

    info!(
        environment = %config.environment,
        sample_rate = config.sample_rate,
        "Initializing Sentry error tracking"
    );

    let guard = sentry::init((
        dsn.as_str(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            environment: Some(config.environment.clone().into()),
            sample_rate: config.sample_rate,
            attach_stacktrace: true,
            send_default_pii: false,
            ..Default::default()
        },
    ));

    if guard.is_enabled() {
        info!("Sentry error tracking initialized successfully");
        Some(guard)
    } else {
        warn!("Sentry initialization failed or is disabled");
        None
    }
}

pub fn capture_error_with_context(
    error: &RouterError,
    request_id: &str,
    client_api_key: &str,
    route: &str,
    provider: Option<&str>,
) {
    // Only capture if Sentry client is initialized
    let client = sentry::Hub::current().client();
    if client.is_none() {
        return;
    }

    sentry::configure_scope(|scope| {
        scope.set_tag("request_id", request_id);
        scope.set_tag("route", route);
        scope.set_tag("error_type", error_type_tag(error));

        if let Some(prov) = provider {
            scope.set_tag("provider", prov);
        }

        // Anonymize API key - only show first 8 chars
        let anonymized_key = if client_api_key.len() > 8 {
            format!("{}...", &client_api_key[..8])
        } else {
            "***".to_string()
        };
        scope.set_extra("api_key_prefix", anonymized_key.into());

        scope.set_context(
            "error_details",
            sentry::protocol::Context::Other(sentry::protocol::Map::from_iter([
                ("error_message".to_string(), error.to_string().into()),
                ("route".to_string(), route.into()),
            ])),
        );
    });

    let level = match error {
        RouterError::Upstream(_) => Level::Warning,
        RouterError::Tls(_) => Level::Error,
        RouterError::ConfigRead(_) | RouterError::ConfigParse(_) => Level::Error,
        RouterError::BadRequest(_) => Level::Info,
        _ => Level::Error,
    };

    sentry::capture_event(Event {
        message: Some(format!("Router Error: {}", error)),
        level,
        ..Default::default()
    });
}

pub fn track_upstream_failure(provider: &str, error: &RouterError) {
    let key = provider.to_string();
    
    let should_alert = UPSTREAM_FAILURE_TRACKER
        .entry(key.clone())
        .or_insert_with(|| UpstreamFailureInfo {
            count: AtomicU64::new(0),
            first_failure: Instant::now(),
            last_alerted: None,
        })
        .value_mut()
        .register_failure();

    if should_alert {
        alert_repeated_upstream_failures(&key, error);
    }

    // Cleanup old entries periodically
    cleanup_old_failure_trackers();
}

impl UpstreamFailureInfo {
    fn register_failure(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.first_failure);

        // Reset counter if window expired
        if elapsed > Duration::from_secs(UPSTREAM_FAILURE_WINDOW_SECS) {
            self.count.store(1, Ordering::SeqCst);
            self.first_failure = now;
            self.last_alerted = None;
            return false;
        }

        let count = self.count.fetch_add(1, Ordering::SeqCst) + 1;

        // Alert if threshold reached and haven't alerted recently
        if count >= UPSTREAM_FAILURE_THRESHOLD {
            let should_alert = match self.last_alerted {
                None => true,
                Some(last) => now.duration_since(last) > Duration::from_secs(60),
            };

            if should_alert {
                self.last_alerted = Some(now);
                return true;
            }
        }

        false
    }
}

fn alert_repeated_upstream_failures(provider: &str, error: &RouterError) {
    error!(
        provider = %provider,
        error = %error,
        "ALERT: Repeated upstream failures detected"
    );

    // Only send alert if Sentry client is initialized
    let client = sentry::Hub::current().client();
    if client.is_some() {
        sentry::configure_scope(|scope| {
            scope.set_tag("alert_type", "repeated_upstream_failures");
            scope.set_tag("provider", provider);
            scope.set_level(Some(Level::Error));
        });

        sentry::capture_message(
            &format!(
                "Repeated upstream failures for provider '{}': {} failures in last {} seconds",
                provider, UPSTREAM_FAILURE_THRESHOLD, UPSTREAM_FAILURE_WINDOW_SECS
            ),
            Level::Error,
        );
    }
}

fn cleanup_old_failure_trackers() {
    let cutoff = Instant::now() - Duration::from_secs(UPSTREAM_FAILURE_WINDOW_SECS * 2);
    
    UPSTREAM_FAILURE_TRACKER.retain(|_, info| {
        info.first_failure > cutoff
    });
}

fn error_type_tag(error: &RouterError) -> &'static str {
    match error {
        RouterError::Url(_) => "url_error",
        RouterError::Io(_) => "io_error",
        RouterError::ConfigRead(_) => "config_read_error",
        RouterError::ConfigParse(_) => "config_parse_error",
        RouterError::Json(_) => "json_error",
        RouterError::Upstream(_) => "upstream_error",
        RouterError::Tls(_) => "tls_error",
        RouterError::BadRequest(_) => "bad_request",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentry_config_from_env_disabled_by_default() {
        let config = SentryConfig::from_env();
        assert!(!config.enabled);
        assert!(config.dsn.is_none());
    }

    #[test]
    fn sentry_config_sample_rate_defaults_to_one() {
        let config = SentryConfig::from_env();
        assert_eq!(config.sample_rate, 1.0);
    }

    #[test]
    fn sentry_config_environment_defaults_to_production() {
        let config = SentryConfig::from_env();
        assert_eq!(config.environment, "production");
    }

    #[test]
    fn error_type_tag_returns_correct_tags() {
        assert_eq!(
            error_type_tag(&RouterError::Url("test".to_string())),
            "url_error"
        );
        assert_eq!(
            error_type_tag(&RouterError::Upstream("test".to_string())),
            "upstream_error"
        );
        assert_eq!(
            error_type_tag(&RouterError::BadRequest("test".to_string())),
            "bad_request"
        );
    }

    #[test]
    fn upstream_failure_info_resets_after_window() {
        let mut info = UpstreamFailureInfo {
            count: AtomicU64::new(10),
            first_failure: Instant::now() - Duration::from_secs(UPSTREAM_FAILURE_WINDOW_SECS + 1),
            last_alerted: None,
        };

        let should_alert = info.register_failure();
        assert!(!should_alert);
        assert_eq!(info.count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn upstream_failure_info_alerts_at_threshold() {
        let mut info = UpstreamFailureInfo {
            count: AtomicU64::new(UPSTREAM_FAILURE_THRESHOLD - 1),
            first_failure: Instant::now(),
            last_alerted: None,
        };

        let should_alert = info.register_failure();
        assert!(should_alert);
    }

    #[test]
    fn upstream_failure_info_throttles_alerts() {
        let mut info = UpstreamFailureInfo {
            count: AtomicU64::new(UPSTREAM_FAILURE_THRESHOLD),
            first_failure: Instant::now(),
            last_alerted: Some(Instant::now()),
        };

        let should_alert = info.register_failure();
        assert!(!should_alert);
    }
}
