use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum StorageConfig {
    File(FileStorageConfig),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileStorageConfig {
    pub path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ArchiveConfig {
    #[serde(default = "default_path")]
    pub path: String,

    pub name: String,
    pub storage: StorageConfig,
}

fn default_path() -> String {
    "..".to_string()
}
