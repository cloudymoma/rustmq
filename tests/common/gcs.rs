use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct GcsTestConfig {
    pub project_id: String,
    pub bucket_name: String,
    pub credentials_path: Option<String>,
}

impl GcsTestConfig {
    pub fn load() -> Option<Self> {
        let path = Path::new("gcs-test.yaml");
        if !path.exists() {
            return None;
        }

        let content = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("Failed to read gcs-test.yaml: {}", e));
        let config: GcsTestConfig = serde_yaml::from_str(&content)
            .unwrap_or_else(|e| panic!("Failed to parse gcs-test.yaml: {}", e));

        Some(config)
    }
}
