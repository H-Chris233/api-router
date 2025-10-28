use api_router::PoolConfig;
use std::time::Duration;

#[test]
fn test_connection_pool_config_defaults() {
    let config = PoolConfig::default();
    assert_eq!(config.max_size, 10);
    assert_eq!(config.idle_timeout, Duration::from_secs(60));
}

#[test]
fn test_connection_pool_config_custom() {
    let config = PoolConfig {
        max_size: 20,
        idle_timeout: Duration::from_secs(120),
    };
    assert_eq!(config.max_size, 20);
    assert_eq!(config.idle_timeout, Duration::from_secs(120));
}
