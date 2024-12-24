use crate::error::DockerError;
use bollard::container::LogsOptions;
use bollard::exec::{CreateExecOptions, StartExecOptions};
use bollard::network::CreateNetworkOptions;
use bollard::secret::{Ipam, IpamConfig, NetworkCreateResponse};
use bollard::volume::{CreateVolumeOptions, ListVolumesOptions};
use futures_util::TryStreamExt;
use ipnet::{IpNet, Ipv4Net};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;

use super::DockerBuilder;

// Network Management
impl DockerBuilder {
    pub async fn find_available_subnet(&self) -> Result<(String, String), DockerError> {
        let networks = self.client.list_networks::<String>(None).await?;
        let used_subnets: Vec<IpNet> = networks
            .iter()
            .filter_map(|n| n.ipam.as_ref())
            .filter_map(|ipam| ipam.config.as_ref())
            .flatten()
            .filter_map(|config| config.subnet.as_ref())
            .filter_map(|s| s.parse().ok())
            .collect();

        // Try different private network ranges
        let ranges = [
            (Ipv4Addr::new(10, 0, 0, 0), 8),     // 10.0.0.0/8
            (Ipv4Addr::new(172, 16, 0, 0), 12),  // 172.16.0.0/12
            (Ipv4Addr::new(192, 168, 0, 0), 16), // 192.168.0.0/16
        ];

        for (base_addr, prefix_len) in ranges {
            // Try subdividing the network into /24 subnets
            for third_octet in 0..=255 {
                let subnet_addr =
                    Ipv4Addr::new(base_addr.octets()[0], base_addr.octets()[1], third_octet, 0);

                let subnet = Ipv4Net::new(subnet_addr, 24).map_err(|e| {
                    DockerError::NetworkCreationError(format!("Invalid subnet: {}", e))
                })?;
                let subnet_net = IpNet::V4(subnet);

                // Check for overlaps
                if !used_subnets.iter().any(|used| {
                    used.contains(&subnet_net.addr()) || subnet_net.contains(&used.addr())
                }) {
                    // Use the first available host address as gateway
                    let gateway = Ipv4Addr::new(
                        subnet_addr.octets()[0],
                        subnet_addr.octets()[1],
                        subnet_addr.octets()[2],
                        1,
                    );

                    return Ok((subnet.to_string(), gateway.to_string()));
                }
            }
        }

        Err(DockerError::NetworkCreationError(
            "No available subnet found".to_string(),
        ))
    }

    pub async fn create_network(
        &self,
        name: &str,
        subnet: &str,
        gateway: &str,
    ) -> Result<NetworkCreateResponse, DockerError> {
        // Validate subnet format
        let subnet_net = IpNet::from_str(subnet).map_err(|e| {
            DockerError::NetworkCreationError(format!("Invalid subnet format: {}", e))
        })?;

        // Validate gateway is within subnet
        let gateway_ip = std::net::IpAddr::from_str(gateway).map_err(|e| {
            DockerError::NetworkCreationError(format!("Invalid gateway format: {}", e))
        })?;
        if !subnet_net.contains(&gateway_ip) {
            return Err(DockerError::NetworkCreationError(
                "Gateway must be within subnet range".to_string(),
            ));
        }

        // Check for existing networks with same name
        let networks = self.list_networks().await?;
        if networks.contains(&name.to_string()) {
            return Err(DockerError::NetworkCreationError(format!(
                "Network {} already exists",
                name
            )));
        }

        let ipam = Ipam {
            driver: Some("default".to_string()),
            config: Some(vec![IpamConfig {
                subnet: Some(subnet.to_string()),
                gateway: Some(gateway.to_string()),
                ip_range: None,
                auxiliary_addresses: None,
            }]),
            options: None,
        };

        self.client
            .create_network(CreateNetworkOptions {
                name,
                driver: "bridge",
                ipam,
                check_duplicate: true,
                internal: false,
                attachable: true,
                ingress: false,
                ..Default::default()
            })
            .await
            .map_err(|e| {
                if e.to_string().contains("Pool overlaps") {
                    DockerError::NetworkCreationError(
                        "Subnet overlaps with existing network".into(),
                    )
                } else {
                    DockerError::BollardError(e)
                }
            })
    }

    pub async fn create_network_with_retry(
        &self,
        name: &str,
        max_retries: usize,
        initial_delay: Duration,
    ) -> Result<NetworkCreateResponse, DockerError> {
        let mut delay = initial_delay;
        let mut last_error = None;

        for attempt in 0..max_retries {
            match self.find_available_subnet().await {
                Ok((subnet, gateway)) => {
                    match self.create_network(name, &subnet, &gateway).await {
                        Ok(response) => return Ok(response),
                        Err(e) => {
                            last_error = Some(e);
                            if attempt < max_retries - 1 {
                                sleep(delay).await;
                                delay *= 2; // Exponential backoff
                                continue;
                            }
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries - 1 {
                        sleep(delay).await;
                        delay *= 2;
                        continue;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            DockerError::NetworkCreationError("Failed to create network after retries".into())
        }))
    }

    pub async fn remove_network(&self, name: &str) -> Result<(), DockerError> {
        // Check if network exists before trying to remove it
        let networks = self.list_networks().await?;
        if !networks.contains(&name.to_string()) {
            return Ok(());
        }

        self.client
            .remove_network(name)
            .await
            .map_err(DockerError::BollardError)
    }

    pub async fn list_networks(&self) -> Result<Vec<String>, DockerError> {
        let networks = self
            .client
            .list_networks::<String>(None)
            .await
            .map_err(DockerError::BollardError)?;

        Ok(networks.into_iter().filter_map(|n| n.name).collect())
    }
}

// Volume Management
impl DockerBuilder {
    pub async fn create_volume(&self, name: &str) -> Result<(), DockerError> {
        self.client
            .create_volume(CreateVolumeOptions {
                name,
                driver: "local",
                ..Default::default()
            })
            .await
            .map_err(DockerError::BollardError)?;

        Ok(())
    }

    pub async fn remove_volume(&self, name: &str) -> Result<(), DockerError> {
        self.client
            .remove_volume(name, None)
            .await
            .map_err(DockerError::BollardError)
    }

    pub async fn list_volumes(&self) -> Result<Vec<String>, DockerError> {
        let volumes = self
            .client
            .list_volumes(None::<ListVolumesOptions<String>>)
            .await
            .map_err(DockerError::BollardError)?;

        Ok(volumes
            .volumes
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| Some(v.name))
            .collect())
    }
}

// Container Logs and Exec
impl DockerBuilder {
    pub async fn get_container_logs(&self, container_id: &str) -> Result<String, DockerError> {
        let mut output = String::new();
        let mut stream = self.client.logs(
            container_id,
            Some(LogsOptions::<String> {
                stdout: true,
                stderr: true,
                timestamps: true,
                follow: false,
                tail: "all".to_string(),
                ..Default::default()
            }),
        );

        while let Some(log) = stream.try_next().await.map_err(DockerError::BollardError)? {
            output.push_str(&log.to_string());
            output.push('\n');
        }

        Ok(output)
    }

    pub async fn exec_in_container(
        &self,
        container_id: &str,
        cmd: Vec<&str>,
        env: Option<HashMap<String, String>>,
    ) -> Result<String, DockerError> {
        let exec = self
            .client
            .create_exec(container_id, CreateExecOptions::<String> {
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                cmd: Some(cmd.into_iter().map(|c| c.to_string()).collect()),
                env: env.map(|e| e.into_iter().map(|(k, v)| format!("{}={}", k, v)).collect()),
                ..Default::default()
            })
            .await
            .map_err(DockerError::BollardError)?;

        let output = self
            .client
            .start_exec(&exec.id, None::<StartExecOptions>)
            .await
            .map_err(DockerError::BollardError)?;

        match output {
            bollard::exec::StartExecResults::Attached { mut output, .. } => {
                let mut bytes = Vec::new();
                while let Some(chunk) =
                    output.try_next().await.map_err(DockerError::BollardError)?
                {
                    bytes.extend_from_slice(&chunk.into_bytes());
                }
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
            _ => Ok(String::new()),
        }
    }

    pub async fn wait_for_container(&self, container_id: &str) -> Result<(), DockerError> {
        let mut retries = 5;
        while retries > 0 {
            let inspect = self
                .client
                .inspect_container(container_id, None)
                .await
                .map_err(DockerError::BollardError)?;

            if let Some(state) = inspect.state {
                if let Some(running) = state.running {
                    if running {
                        return Ok(());
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            retries -= 1;
        }
        Err(DockerError::ContainerNotRunning(container_id.to_string()))
    }
}
