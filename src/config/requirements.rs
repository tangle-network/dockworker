use crate::error::DockerError;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(feature = "docker")]
use bollard::service::HostConfig;

#[cfg(feature = "docker")]
use sysinfo::{DiskExt, System, SystemExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemRequirements {
    pub min_memory_gb: u64,
    pub min_disk_gb: u64,
    pub min_bandwidth_mbps: u64,
    pub required_ports: Vec<u16>,
    pub data_directory: String,
    // Resource limit fields
    pub cpu_limit: Option<f64>,             // Number of CPUs
    pub memory_limit: Option<String>,       // e.g., "1G", "512M"
    pub memory_swap: Option<String>,        // Total memory including swap
    pub memory_reservation: Option<String>, // Soft limit
    pub cpu_shares: Option<i64>,            // CPU shares (relative weight)
    pub cpuset_cpus: Option<String>,        // CPUs in which to allow execution (0-3, 0,1)
}

#[cfg(feature = "docker")]
impl SystemRequirements {
    pub fn check(&self) -> Result<(), DockerError> {
        let mut sys = System::new_all();
        sys.refresh_all();

        // Check memory
        let total_memory = sys.total_memory() / 1024 / 1024 / 1024; // Convert to GB
        if total_memory < self.min_memory_gb {
            return Err(DockerError::ValidationError(format!(
                "Insufficient memory: {} GB available, {} GB required",
                total_memory, self.min_memory_gb
            )));
        }

        // Check memory limits if specified
        if let Some(limit) = &self.memory_limit {
            let limit_bytes = parse_memory_string(limit)?;
            let total_bytes = (total_memory as i64) * 1024 * 1024 * 1024;
            if limit_bytes > total_bytes {
                return Err(DockerError::ValidationError(format!(
                    "Memory limit {} exceeds available memory {}GB",
                    limit, total_memory
                )));
            }
        }

        // Check disk space
        let data_path = Path::new(&self.data_directory);
        if let Some(disk) = sys
            .disks()
            .iter()
            .find(|disk| data_path.starts_with(disk.mount_point().to_string_lossy().as_ref()))
        {
            let available_gb = disk.available_space() / 1024 / 1024 / 1024;
            if available_gb < self.min_disk_gb {
                return Err(DockerError::ValidationError(format!(
                    "Insufficient disk space: {} GB available, {} GB required",
                    available_gb, self.min_disk_gb
                )));
            }
        }

        // Check if ports are available
        for port in &self.required_ports {
            if !is_port_available(*port) {
                return Err(DockerError::ValidationError(format!(
                    "Port {} is already in use",
                    port
                )));
            }
        }

        Ok(())
    }

    pub fn to_host_config(&self) -> HostConfig {
        let mut host_config = HostConfig::default();

        // Set resource limits
        if let Some(memory) = &self.memory_limit {
            host_config.memory = parse_memory_string(memory).ok();
        }
        if let Some(swap) = &self.memory_swap {
            host_config.memory_swap = parse_memory_string(swap).ok();
        }
        if let Some(reservation) = &self.memory_reservation {
            host_config.memory_reservation = parse_memory_string(reservation).ok();
        }
        host_config.cpu_shares = self.cpu_shares;
        host_config.cpuset_cpus = self.cpuset_cpus.clone();
        if let Some(cpu) = self.cpu_limit {
            host_config.nano_cpus = Some((cpu * 1e9) as i64);
        }

        host_config
    }
}

fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

// Helper function to parse memory strings like "1G", "512M" into bytes
pub fn parse_memory_string(memory: &str) -> Result<i64, DockerError> {
    let len = memory.len();
    let (num, unit) = memory.split_at(len - 1);
    let base = num.parse::<i64>().map_err(|_| {
        DockerError::InvalidResourceLimit(format!("Invalid memory value: {}", memory))
    })?;

    match unit.to_uppercase().as_str() {
        "K" => Ok(base * 1024),
        "M" => Ok(base * 1024 * 1024),
        "G" => Ok(base * 1024 * 1024 * 1024),
        _ => Err(DockerError::InvalidResourceLimit(format!(
            "Invalid memory unit: {}",
            unit
        ))),
    }
}
