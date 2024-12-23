use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeConfig {
    pub(crate) version: String,
    pub(crate) services: HashMap<String, ServiceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub(crate) image: Option<String>,
    pub(crate) build: Option<BuildConfig>,
    pub(crate) ports: Option<Vec<String>>,
    pub(crate) environment: Option<HashMap<String, String>>,
    pub(crate) volumes: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub(crate) context: String,
    pub(crate) dockerfile: Option<String>,
} 