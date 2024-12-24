use crate::DockerBuilder;
use crate::error::DockerError;
use bollard::container::LogsOptions;
use bollard::exec::{CreateExecOptions, StartExecOptions};
use bollard::network::CreateNetworkOptions;
use bollard::volume::{CreateVolumeOptions, ListVolumesOptions};
use futures_util::{StreamExt, TryStreamExt};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

impl DockerBuilder {
    // Network Management
    pub async fn create_network_with_retry(
        &self,
        name: &str,
        max_retries: u32,
        initial_delay: Duration,
    ) -> Result<(), DockerError> {
        let mut delay = initial_delay;
        let mut attempts = 0;

        while attempts < max_retries {
            let result = self
                .get_client()
                .create_network(CreateNetworkOptions {
                    name: name.to_string(),
                    driver: "bridge".to_string(),
                    ..Default::default()
                })
                .await;

            match result {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if attempts == max_retries - 1 {
                        return Err(DockerError::BollardError(e));
                    }
                    attempts += 1;
                    sleep(delay).await;
                    delay *= 2;
                }
            }
        }

        Ok(())
    }

    pub async fn remove_network(&self, name: &str) -> Result<(), DockerError> {
        self.get_client()
            .remove_network(name)
            .await
            .map_err(DockerError::BollardError)
    }

    pub async fn pull_image(&self, image: &str, platform: Option<&str>) -> Result<(), DockerError> {
        let mut pull_stream = self.client.create_image(
            Some(bollard::image::CreateImageOptions {
                from_image: image,
                platform: platform.unwrap_or("linux/amd64"),
                ..Default::default()
            }),
            None,
            None,
        );

        while let Some(pull_result) = pull_stream.next().await {
            match pull_result {
                Ok(_) => continue,
                Err(e) => return Err(DockerError::BollardError(e)),
            }
        }

        Ok(())
    }

    pub async fn list_networks(&self) -> Result<Vec<String>, DockerError> {
        let networks = self
            .get_client()
            .list_networks::<String>(None)
            .await
            .map_err(DockerError::BollardError)?;

        Ok(networks.into_iter().filter_map(|n| n.name).collect())
    }

    // Volume Management
    pub async fn create_volume(&self, name: &str) -> Result<(), DockerError> {
        self.get_client()
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
        self.get_client()
            .remove_volume(name, None)
            .await
            .map_err(DockerError::BollardError)
    }

    pub async fn list_volumes(&self) -> Result<Vec<String>, DockerError> {
        let volumes = self
            .get_client()
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

    // Container Management
    pub async fn wait_for_container(&self, container_id: &str) -> Result<(), DockerError> {
        let mut retries = 5;
        while retries > 0 {
            let inspect = self
                .get_client()
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
            sleep(Duration::from_millis(500)).await;
            retries -= 1;
        }
        Err(DockerError::ValidationError(format!(
            "Container {} not running after retries",
            container_id
        )))
    }

    pub async fn get_container_logs(&self, container_id: &str) -> Result<String, DockerError> {
        let mut output = String::new();
        let mut stream = self.get_client().logs(
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
            .get_client()
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
            .get_client()
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
}
