use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeConfig {
    pub(crate) version: String,
    pub(crate) services: HashMap<String, ServiceConfig>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    #[serde(default)]
    pub(crate) image: Option<String>,
    #[serde(default)]
    pub(crate) build: Option<BuildConfig>,
    #[serde(default)]
    pub(crate) ports: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) environment: Option<HashMap<String, String>>,
    #[serde(default)]
    pub(crate) volumes: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) networks: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) resources: Option<ResourceLimits>,
    #[serde(default)]
    pub(crate) depends_on: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) healthcheck: Option<HealthCheck>,
    #[serde(default)]
    pub(crate) restart: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub(crate) context: String,
    pub(crate) dockerfile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub(crate) cpu_limit: Option<f64>,             // Number of CPUs
    pub(crate) memory_limit: Option<String>,       // e.g., "1G", "512M"
    pub(crate) memory_swap: Option<String>,        // Total memory including swap
    pub(crate) memory_reservation: Option<String>, // Soft limit
    pub(crate) cpus_shares: Option<i64>,           // CPU shares (relative weight)
    pub(crate) cpuset_cpus: Option<String>,        // CPUs in which to allow execution (0-3, 0,1)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub(crate) test: Vec<String>, // The test to perform. Possible values: [], ["NONE"], ["CMD", args...], ["CMD-SHELL", command]
    pub(crate) interval: Option<i64>, // Time between checks in nanoseconds. 0 or >= 1000000 (1ms). 0 means inherit.
    pub(crate) timeout: Option<i64>, // Time to wait before check is hung. 0 or >= 1000000 (1ms). 0 means inherit.
    pub(crate) retries: Option<i64>, // Number of consecutive failures before unhealthy. 0 means inherit.
    pub(crate) start_period: Option<i64>, // Container init period before retries countdown in ns. 0 or >= 1000000 (1ms). 0 means inherit.
    pub(crate) start_interval: Option<i64>, // Time between checks during start period in ns. 0 or >= 1000000 (1ms). 0 means inherit.
}
