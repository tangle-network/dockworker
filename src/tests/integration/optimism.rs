use crate::DockerBuilder;
use crate::config::compose::ComposeConfig;
use crate::config::volume::VolumeType;
use crate::parser::compose::ComposeParser;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use uuid::Uuid;

const OPTIMISM_FIXTURES: &str = "fixtures/simple-optimism-node";
const TEST_BASE_DIR: &str = "test-data";

pub struct OptimismTestContext {
    pub builder: DockerBuilder,
    pub config: ComposeConfig,
    pub test_dir: PathBuf,
    pub network_name: String,
}

impl OptimismTestContext {
    pub async fn new() -> Result<Self, crate::error::DockerError> {
        let builder = DockerBuilder::new()?;

        // Read and parse the compose file
        let compose_path = Path::new(OPTIMISM_FIXTURES).join("docker-compose.yml");
        let compose_content = fs::read_to_string(&compose_path).await.map_err(|e| {
            crate::error::DockerError::ValidationError(format!(
                "Failed to read compose file: {}",
                e
            ))
        })?;
        let config = ComposeParser::parse(&compose_content)?;

        // Create test directory under the common test base
        let test_dir = PathBuf::from(TEST_BASE_DIR)
            .join("optimism")
            .join(format!("test-{}", Uuid::new_v4()));
        let network_name = format!("optimism-test-{}", Uuid::new_v4());

        Ok(Self {
            builder,
            config,
            test_dir,
            network_name,
        })
    }

    pub fn get_compose_path(&self) -> PathBuf {
        self.test_dir.join("docker-compose.yml")
    }

    pub fn get_env_path(&self) -> PathBuf {
        Path::new("src/tests/integration/optimism_env.env").to_path_buf()
    }

    pub async fn setup_directories(&self) -> Result<(), crate::error::DockerError> {
        // Create test directory
        fs::create_dir_all(&self.test_dir).await.map_err(|e| {
            crate::error::DockerError::ValidationError(format!(
                "Failed to create test directory: {}",
                e
            ))
        })?;

        // Create required directories
        let dirs = [
            "docker/prometheus",
            "docker/grafana/provisioning/",
            "docker/grafana/dashboards/simple_node_dashboard.json",
            "envs/op-mainnet/config",
            "docker/influxdb/influx_init.iql",
            "docker/dockerfiles",
            "envs/common",
            "envs/op-mainnet",
            "shared",
            "downloads",
            "torrents/op-mainnet",
            "scripts",
        ];

        for dir in dirs {
            let dir_path = self.test_dir.join(dir);
            println!("Created directory: {}", dir_path.display());
            fs::create_dir_all(&dir_path).await.map_err(|e| {
                crate::error::DockerError::ValidationError(format!(
                    "Failed to create directory {}: {}",
                    dir_path.display(),
                    e
                ))
            })?;
        }

        // Copy docker-compose.yml
        let compose_src = Path::new(OPTIMISM_FIXTURES).join("docker-compose.yml");
        let compose_dst = self.test_dir.join("docker-compose.yml");
        println!(
            "Copying {} to {}",
            compose_src.display(),
            compose_dst.display()
        );
        fs::copy(&compose_src, &compose_dst).await.map_err(|e| {
            crate::error::DockerError::ValidationError(format!(
                "Failed to copy {} to {}: {}",
                compose_src.display(),
                compose_dst.display(),
                e
            ))
        })?;

        // Copy Dockerfile.bedrock-init
        let dockerfile_src =
            Path::new(OPTIMISM_FIXTURES).join("docker/dockerfiles/Dockerfile.bedrock-init");
        let dockerfile_dst = self
            .test_dir
            .join("docker/dockerfiles/Dockerfile.bedrock-init");
        println!(
            "Copying {} to {}",
            dockerfile_src.display(),
            dockerfile_dst.display()
        );
        fs::copy(&dockerfile_src, &dockerfile_dst)
            .await
            .map_err(|e| {
                crate::error::DockerError::ValidationError(format!(
                    "Failed to copy {} to {}: {}",
                    dockerfile_src.display(),
                    dockerfile_dst.display(),
                    e
                ))
            })?;

        // Copy environment files
        let env_files = [
            ("envs/op-mainnet/op-geth.env", "envs/op-mainnet/op-geth.env"),
            ("envs/op-mainnet/op-node.env", "envs/op-mainnet/op-node.env"),
            ("envs/common/l2geth.env", "envs/common/l2geth.env"),
            ("envs/common/healthcheck.env", "envs/common/healthcheck.env"),
            ("envs/common/grafana.env", "envs/common/grafana.env"),
            ("envs/common/influxdb.env", "envs/common/influxdb.env"),
        ];

        for (src, dst) in env_files {
            let src_path = Path::new(OPTIMISM_FIXTURES).join(src);
            let dst_path = self.test_dir.join(dst);

            // Create parent directory if it doesn't exist
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent).await.map_err(|e| {
                    crate::error::DockerError::ValidationError(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }

            println!("Copying {} to {}", src_path.display(), dst_path.display());
            fs::copy(&src_path, &dst_path).await.map_err(|e| {
                crate::error::DockerError::ValidationError(format!(
                    "Failed to copy {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                ))
            })?;
        }

        // Copy scripts directory
        let scripts_src = Path::new(OPTIMISM_FIXTURES).join("scripts");
        let scripts_dst = self.test_dir.join("scripts");
        println!(
            "Copying directory {} to {}",
            scripts_src.display(),
            scripts_dst.display()
        );

        // Create scripts directory
        fs::create_dir_all(&scripts_dst).await.map_err(|e| {
            crate::error::DockerError::ValidationError(format!(
                "Failed to create scripts directory: {}",
                e
            ))
        })?;

        // Copy all files from scripts directory
        let mut dir_entries = fs::read_dir(&scripts_src).await.map_err(|e| {
            crate::error::DockerError::ValidationError(format!(
                "Failed to read scripts directory: {}",
                e
            ))
        })?;

        while let Some(entry) = dir_entries.next_entry().await.map_err(|e| {
            crate::error::DockerError::ValidationError(format!(
                "Failed to read directory entry: {}",
                e
            ))
        })? {
            let src_path = entry.path();
            let dst_path = scripts_dst.join(src_path.file_name().unwrap());

            println!("Copying {} to {}", src_path.display(), dst_path.display());
            fs::copy(&src_path, &dst_path).await.map_err(|e| {
                crate::error::DockerError::ValidationError(format!(
                    "Failed to copy {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                ))
            })?;

            // Make scripts executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dst_path).await?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dst_path, perms).await.map_err(|e| {
                    crate::error::DockerError::ValidationError(format!(
                        "Failed to make {} executable: {}",
                        dst_path.display(),
                        e
                    ))
                })?;
            }
        }

        Ok(())
    }

    pub async fn cleanup(&self) -> Result<(), crate::error::DockerError> {
        // Remove test directory
        fs::remove_dir_all(&self.test_dir).await.map_err(|e| {
            crate::error::DockerError::ValidationError(format!(
                "Failed to cleanup test directory: {}",
                e
            ))
        })?;

        // Remove network
        if let Err(e) = self.builder.remove_network(&self.network_name).await {
            println!("Warning: Failed to remove network: {}", e);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    async fn test_optimism_node_deployment() -> Result<(), Box<dyn std::error::Error>> {
        let mut ctx = OptimismTestContext::new().await?;

        // Setup test environment
        ctx.setup_directories().await?;

        // Create Docker network
        ctx.builder
            .create_network_with_retry(&ctx.network_name, 5, Duration::from_millis(100))
            .await?;

        // Pull required base images
        ctx.builder
            .pull_image("ubuntu:22.04", Some("linux/amd64"))
            .await?;

        // Fix volume paths to use absolute paths
        let current_dir = env::current_dir()?;
        for service in ctx.config.services.values_mut() {
            if let Some(volumes) = &mut service.volumes {
                for volume in volumes.iter_mut() {
                    if let VolumeType::Bind { source, .. } = volume {
                        if source.starts_with("./") {
                            *source = current_dir
                                .join(&ctx.test_dir)
                                .join(source.strip_prefix("./").unwrap())
                                .to_string_lossy()
                                .to_string();
                        }
                    }
                }
            }
        }

        // Load and parse the compose file with environment variables
        let compose_path = ctx.get_compose_path();
        let env_path = ctx.get_env_path();
        ctx.config = ComposeParser::from_file_with_env(&compose_path, &env_path).await?;

        // Deploy all services using our library's functionality
        let container_ids = ctx.builder.deploy_compose(&mut ctx.config).await?;

        // Verify services are running
        for (service_name, container_id) in container_ids {
            println!(
                "Service {} deployed with container ID: {}",
                service_name, container_id
            );
        }

        // Cleanup
        ctx.cleanup().await?;

        Ok(())
    }
}
