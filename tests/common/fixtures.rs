use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

use super::project_root;

pub struct ConfigFixture {
    value: Value,
}

impl ConfigFixture {
    pub fn provider(name: &str) -> Self {
        let path = project_root()
            .join("transformer")
            .join(format!("{}.json", name));
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", path.display(), e));
        let value: Value = serde_json::from_str(&contents)
            .unwrap_or_else(|e| panic!("invalid JSON in {}: {}", path.display(), e));
        Self { value }
    }

    pub fn from_value(value: Value) -> Self {
        Self { value }
    }

    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.value["baseUrl"] = json!(base_url);
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.value["port"] = json!(port);
        self
    }

    pub fn set_endpoint_rate_limit(mut self, endpoint: &str, rpm: u32, burst: u32) -> Self {
        let endpoints = self
            .value
            .get_mut("endpoints")
            .and_then(Value::as_object_mut)
            .expect("fixture missing endpoints object");
        let endpoint_config = endpoints
            .entry(endpoint.to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        let endpoint_obj = endpoint_config
            .as_object_mut()
            .expect("endpoint config should be object");
        endpoint_obj.insert(
            "rateLimit".into(),
            json!({
                "requestsPerMinute": rpm,
                "burst": burst,
            }),
        );
        self
    }

    pub fn into_temp_file(self) -> TempConfigFile {
        let mut file = NamedTempFile::new().expect("failed to create temp config file");
        serde_json::to_writer_pretty(&mut file, &self.value)
            .expect("failed to write config fixture");
        file.flush().expect("failed to flush config fixture");
        TempConfigFile { file }
    }

    pub fn into_value(self) -> Value {
        self.value
    }
}

pub struct TempConfigFile {
    file: NamedTempFile,
}

impl TempConfigFile {
    pub fn path(&self) -> &Path {
        self.file.path()
    }

    pub fn rewrite(&self, value: &Value) {
        let contents = serde_json::to_string_pretty(value)
            .expect("failed to serialize config fixture");
        fs::write(self.file.path(), contents).expect("failed to overwrite config fixture");
    }
}
