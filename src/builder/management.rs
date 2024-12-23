use crate::error::DockerError;
use bollard::container::LogsOptions;
use bollard::exec::{CreateExecOptions, StartExecOptions};
use bollard::network::CreateNetworkOptions;
use bollard::secret::{Ipam, IpamConfig, NetworkCreateResponse};
use bollard::volume::{CreateVolumeOptions, ListVolumesOptions};
use futures_util::TryStreamExt;
use ipnet::IpNet;
use std::collections::HashMap;
use std::time::Duration;

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

        // Try simple subnet ranges
        for octet2 in [10, 172, 192] {
            for octet3 in 0..255 {
                let subnet = format!("{}.{}.0.0/16", octet2, octet3);
                let gateway = format!("{}.{}.0.1", octet2, octet3);

                // Try to parse the subnet to validate it
                if let Ok(candidate) = subnet.parse::<IpNet>() {
                    if !used_subnets.iter().any(|net| {
                        net.contains(&candidate.addr()) || candidate.contains(&net.addr())
                    }) {
                        return Ok((subnet, gateway));
                    }
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
        subnet: &str,  // No longer optional
        gateway: &str, // No longer optional
    ) -> Result<NetworkCreateResponse, DockerError> {
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
            .map_err(DockerError::BollardError)
    }

    pub async fn create_network_with_retry(
        &self,
        name: &str,
        max_retries: usize,
    ) -> Result<(), DockerError> {
        for attempt in 0..max_retries {
            match self.find_available_subnet().await {
                Ok((subnet, gateway)) => match self.create_network(name, &subnet, &gateway).await {
                    Ok(_) => return Ok(()),
                    Err(_) if attempt < max_retries - 1 => {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                    Err(e) => return Err(e),
                },
                Err(_) if attempt < max_retries - 1 => {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(DockerError::NetworkCreationError(
            "Failed to create network after retries".to_string(),
        ))
    }

    pub async fn remove_network(&self, name: &str) -> Result<(), DockerError> {
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