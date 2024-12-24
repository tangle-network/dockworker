use crate::error::DockerError;
use std::path::Path;

#[cfg(feature = "docker")]
use sysinfo::{DiskExt, System, SystemExt};

#[derive(Debug, Clone)]
pub struct SystemRequirements {
    pub min_memory_gb: u64,
    pub min_disk_gb: u64,
    pub min_bandwidth_mbps: u64,
    pub required_ports: Vec<u16>,
    pub data_directory: String,
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
}

fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}
