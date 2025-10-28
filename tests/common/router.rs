use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::project_root;

pub const TEST_DEFAULT_API_KEY: &str = "integration-default-key";

pub struct RouterProcess {
    child: Child,
}

impl RouterProcess {
    pub fn start(config_path: &Path, port: u16, extra_env: &[(&str, &str)]) -> Self {
        let binary = router_binary_path();
        if !binary.exists() {
            panic!("router binary not found at {:?}", binary);
        }
        let mut command = Command::new(&binary);
        command
            .env("API_ROUTER_CONFIG_PATH", config_path)
            .env("DEFAULT_API_KEY", TEST_DEFAULT_API_KEY)
            .env("RUST_LOG", "warn")
            .current_dir(project_root())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        for (key, value) in extra_env {
            command.env(key, value);
        }

        let mut child = command.spawn().expect("failed to launch router process");

        if !wait_for_router(&mut child, port, Duration::from_secs(5)) {
            let _ = child.kill();
            panic!("router failed to start on port {}", port);
        }

        Self { child }
    }
}

impl Drop for RouterProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

fn router_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_api-router") {
        return PathBuf::from(path);
    }

    let mut path = project_root();
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    path.push("target");
    path.push(profile);
    if cfg!(windows) {
        path.push("api-router.exe");
    } else {
        path.push("api-router");
    }
    path
}

fn wait_for_router(child: &mut Child, port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        if let Some(status) = child.try_wait().expect("failed to poll child process") {
            panic!("router process exited prematurely: {}", status);
        }
        thread::sleep(Duration::from_millis(25));
    }
    false
}
