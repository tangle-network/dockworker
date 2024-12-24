use crate::config::health::HealthCheck;
use reqwest::Client;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

const OPTIMISM_FIXTURES: &str = "fixtures/simple-optimism-node";

/// Helper function to get environment file paths for a specific network
fn get_network_env_files(network: &str) -> Vec<std::path::PathBuf> {
    let base_path = Path::new(OPTIMISM_FIXTURES).join("envs");
    let mut env_files = vec![
        // Network specific env files
        base_path.join(network).join("op-geth.env"),
        base_path.join(network).join("op-node.env"),
        // Common env files
        base_path.join("common").join("grafana.env"),
        base_path.join("common").join("healthcheck.env"),
        base_path.join("common").join("influxdb.env"),
        base_path.join("common").join("l2geth.env"),
    ];

    // Add config files if they exist
    let config_path = base_path.join(network).join("config");
    if config_path.exists() {
        env_files.extend(vec![
            config_path.join("genesis.json"),
            config_path.join("rollup.json"),
        ]);
    }

    env_files
}

pub struct BlockchainNodeHealth {
    pub rpc_endpoint: String,
    pub reference_endpoint: Option<String>,
    pub max_block_delay: u64,
}

impl BlockchainNodeHealth {
    pub async fn check(&self) -> Result<(), crate::error::DockerError> {
        let client = Client::new();

        // Check if node is syncing
        let syncing = self.check_syncing(&client).await?;
        if syncing {
            return Err(crate::error::DockerError::ValidationError(
                "Node is still syncing".to_string(),
            ));
        }

        // Compare with reference node if available
        if let Some(ref_endpoint) = &self.reference_endpoint {
            self.compare_with_reference(&client, ref_endpoint).await?;
        }

        Ok(())
    }

    async fn check_syncing(&self, client: &Client) -> Result<bool, crate::error::DockerError> {
        let response: Value = client
            .post(&self.rpc_endpoint)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "eth_syncing",
                "params": [],
                "id": 1
            }))
            .send()
            .await
            .map_err(|e| {
                crate::error::DockerError::ValidationError(format!("RPC request failed: {}", e))
            })?
            .json()
            .await
            .map_err(|e| {
                crate::error::DockerError::ValidationError(format!("Invalid JSON response: {}", e))
            })?;

        Ok(response["result"] != false)
    }

    async fn compare_with_reference(
        &self,
        client: &Client,
        reference_endpoint: &str,
    ) -> Result<(), crate::error::DockerError> {
        let node_block = self.get_block_number(client, &self.rpc_endpoint).await?;
        let ref_block = self.get_block_number(client, reference_endpoint).await?;

        if ref_block - node_block > self.max_block_delay {
            return Err(crate::error::DockerError::ValidationError(format!(
                "Node is {} blocks behind reference node",
                ref_block - node_block
            )));
        }

        Ok(())
    }

    async fn get_block_number(
        &self,
        client: &Client,
        endpoint: &str,
    ) -> Result<u64, crate::error::DockerError> {
        let response: Value = client
            .post(endpoint)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "eth_blockNumber",
                "params": [],
                "id": 1
            }))
            .send()
            .await
            .map_err(|e| {
                crate::error::DockerError::ValidationError(format!("RPC request failed: {}", e))
            })?
            .json()
            .await
            .map_err(|e| {
                crate::error::DockerError::ValidationError(format!("Invalid JSON response: {}", e))
            })?;

        let hex_str = response["result"].as_str().ok_or_else(|| {
            crate::error::DockerError::ValidationError("Invalid block number response".into())
        })?;

        u64::from_str_radix(&hex_str[2..], 16).map_err(|e| {
            crate::error::DockerError::ValidationError(format!("Invalid block number: {}", e))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{VolumeType, compose::ComposeConfig};
    use crate::parser::compose::ComposeParser;
    use std::collections::HashMap;
    use std::path::Path;

    const OPTIMISM_FIXTURES: &str = "fixtures/simple-optimism-node";

    fn load_env_vars() -> HashMap<String, String> {
        let env_content = std::fs::read_to_string("src/tests/integration/optimism_env.env")
            .expect("Failed to read env file");

        let mut vars = HashMap::new();
        for line in env_content.lines() {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                vars.insert(
                    key.trim().to_string(),
                    value.trim().trim_matches('"').to_string(),
                );
            }
        }
        vars
    }

    #[test]
    fn test_optimism_env_parsing() -> Result<(), Box<dyn std::error::Error>> {
        // Load environment variables
        let env_vars = load_env_vars();

        // Parse the compose file
        let compose_path = Path::new(OPTIMISM_FIXTURES).join("docker-compose.yml");
        let compose_content = std::fs::read_to_string(&compose_path)?;

        // Replace environment variables in compose content
        let mut processed_content = compose_content;
        for (key, value) in &env_vars {
            processed_content = processed_content.replace(&format!("${{{}}}", key), value);
        }

        let config = ComposeParser::parse(&processed_content)?;

        // Verify op-geth configuration
        let op_geth = config
            .services
            .get("op-geth")
            .expect("op-geth service not found");
        assert!(op_geth.volumes.is_some(), "op-geth should have volumes");

        // Verify environment variables were substituted
        let network_name = env_vars
            .get("NETWORK_NAME")
            .expect("NETWORK_NAME not found in env");
        assert!(
            processed_content.contains(network_name),
            "NETWORK_NAME should be substituted in compose file"
        );

        Ok(())
    }

    #[cfg(feature = "docker")]
    #[tokio::test]
    async fn test_optimism_node_deployment() -> Result<(), Box<dyn std::error::Error>> {
        use crate::DockerBuilder;
        use bollard::container::{
            Config, CreateContainerOptions, StartContainerOptions, WaitContainerOptions,
        };
        use bollard::service::HostConfig;
        use futures_util::StreamExt;
        use std::path::PathBuf;

        let builder = DockerBuilder::new()?;

        // Get absolute path to fixtures
        let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixtures_path = workspace_dir.join(OPTIMISM_FIXTURES);

        // Load environment variables
        let env_vars = load_env_vars();

        // Parse the compose file
        let compose_path = fixtures_path.join("docker-compose.yml");
        let compose_content = std::fs::read_to_string(&compose_path)?;

        // Replace environment variables in compose content
        let mut processed_content = compose_content;
        for (key, value) in &env_vars {
            processed_content = processed_content.replace(&format!("${{{}}}", key), value);
        }

        let mut config = ComposeParser::parse(&processed_content)?;

        // Fix relative paths in volumes to be absolute
        for service in config.services.values_mut() {
            if let Some(volumes) = &mut service.volumes {
                for volume in volumes.iter_mut() {
                    if let VolumeType::Bind { source, .. } = volume {
                        if source.is_relative() {
                            *source = fixtures_path
                                .join(source.strip_prefix("./").unwrap_or(source.as_path()));
                        }
                    }
                }
            }
        }

        // Create required directories
        for service in config.services.values() {
            if let Some(volumes) = &service.volumes {
                for volume in volumes {
                    if let VolumeType::Bind { source, .. } = volume {
                        if !source.exists() {
                            println!("Creating directory: {}", source.display());
                            tokio::fs::create_dir_all(source).await?;
                        }
                    }
                }
            }
        }

        // Start with bedrock-init
        let init_service = config
            .services
            .get("bedrock-init")
            .expect("bedrock-init service not found");

        // Create container config
        let container_config = Config {
            image: init_service.image.to_owned(),
            cmd: Some(vec!["/scripts/init-bedrock.sh".to_string()]),
            working_dir: Some("/scripts".to_string()),
            host_config: Some(HostConfig {
                binds: init_service
                    .volumes
                    .as_ref()
                    .map(|vols| vols.iter().map(|v| String::from(v.to_string())).collect()),
                ..Default::default()
            }),
            env: init_service.environment.as_ref().map(|env| {
                env.iter()
                    .map(|(k, v)| String::from(format!("{}={}", k, v)))
                    .collect()
            }),
            ..Default::default()
        };

        println!("Creating container with config: {:?}", container_config);

        // Create and start bedrock-init
        let init_container = builder
            .get_client()
            .create_container(None::<CreateContainerOptions<String>>, container_config)
            .await?;

        println!("Starting container {}", init_container.id);

        // Start the container
        builder
            .get_client()
            .start_container(&init_container.id, None::<StartContainerOptions<String>>)
            .await?;

        // Wait for completion
        let mut wait_stream = builder.get_client().wait_container(
            &init_container.id,
            Some(WaitContainerOptions {
                condition: "not-running",
            }),
        );

        while let Some(wait_result) = wait_stream.next().await {
            match wait_result {
                Ok(exit) => {
                    // Get container logs for debugging
                    let logs = builder.get_container_logs(&init_container.id).await?;
                    println!("Container logs: {}", logs);

                    if exit.status_code != 0 {
                        return Err(format!(
                            "bedrock-init failed with status code: {}. Logs: {}",
                            exit.status_code, logs
                        )
                        .into());
                    }
                    break;
                }
                Err(e) => {
                    return Err(format!("Error waiting for bedrock-init: {}", e).into());
                }
            }
        }

        // Clean up
        builder
            .get_client()
            .remove_container(
                &init_container.id,
                Some(bollard::container::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await?;

        Ok(())
    }
}
