use api_router::error_tracking::{init_sentry, SentryConfig};
use std::env;

#[test]
fn test_sentry_disabled_by_default() {
    // Ensure SENTRY_DSN is not set
    env::remove_var("SENTRY_DSN");
    
    let config = SentryConfig::from_env();
    assert!(!config.enabled);
    assert!(config.dsn.is_none());
    
    // Should return None when disabled
    let guard = init_sentry(&config);
    assert!(guard.is_none());
}

#[test]
fn test_sentry_config_sample_rate() {
    env::remove_var("SENTRY_DSN");
    env::set_var("SENTRY_SAMPLE_RATE", "0.5");
    
    let config = SentryConfig::from_env();
    assert_eq!(config.sample_rate, 0.5);
    
    env::remove_var("SENTRY_SAMPLE_RATE");
}

#[test]
fn test_sentry_config_environment() {
    env::remove_var("SENTRY_DSN");
    env::set_var("SENTRY_ENVIRONMENT", "staging");
    
    let config = SentryConfig::from_env();
    assert_eq!(config.environment, "staging");
    
    env::remove_var("SENTRY_ENVIRONMENT");
}

#[test]
fn test_sentry_config_with_invalid_sample_rate_clamps() {
    env::remove_var("SENTRY_DSN");
    env::set_var("SENTRY_SAMPLE_RATE", "2.5");
    
    let config = SentryConfig::from_env();
    assert_eq!(config.sample_rate, 1.0); // Should clamp to 1.0
    
    env::set_var("SENTRY_SAMPLE_RATE", "-0.5");
    let config = SentryConfig::from_env();
    assert_eq!(config.sample_rate, 0.0); // Should clamp to 0.0
    
    env::remove_var("SENTRY_SAMPLE_RATE");
}
