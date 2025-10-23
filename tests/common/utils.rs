use std::net::TcpListener;
use std::path::PathBuf;

pub fn pick_free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .expect("failed to bind ephemeral port")
        .local_addr()
        .expect("failed to read local addr")
        .port()
}

pub fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
