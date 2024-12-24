use bollard::container::CreateContainerOptions;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

use super::docker_file::is_docker_running;
use crate::DockerBuilder;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_network_management() -> Result<(), Box<dyn std::error::Error>> {
        let builder = DockerBuilder::new()?;

        // Create a unique network name
        let network_name = format!("test_network_{}", Uuid::new_v4());

        // Create network with retries
        let result = timeout(
            Duration::from_secs(30),
            builder.create_network_with_retry(
                &network_name,
                5,                          // max retries
                Duration::from_millis(100), // initial delay
            ),
        )
        .await??;

        assert!(result.id.is_some(), "Network creation should return an ID");

        // Verify network exists
        let networks = timeout(Duration::from_secs(5), builder.list_networks()).await??;
        assert!(
            networks.contains(&network_name),
            "Created network should be in the list"
        );

        // Clean up
        timeout(
            Duration::from_secs(5),
            builder.remove_network(&network_name),
        )
        .await??;

        // Verify removal
        let networks_after = timeout(Duration::from_secs(5), builder.list_networks()).await??;
        assert!(
            !networks_after.contains(&network_name),
            "Removed network should not be in the list"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_network_subnet_validation() -> Result<(), Box<dyn std::error::Error>> {
        let builder = DockerBuilder::new()?;

        // Test finding available subnet
        let (subnet, gateway) = builder.find_available_subnet().await?;

        // Validate subnet format
        assert!(
            subnet.parse::<ipnet::IpNet>().is_ok(),
            "Subnet should be valid CIDR"
        );

        // Validate gateway is in subnet
        let subnet_net = subnet.parse::<ipnet::IpNet>()?;
        let gateway_ip = gateway.parse::<std::net::IpAddr>()?;
        assert!(
            subnet_net.contains(&gateway_ip),
            "Gateway should be in subnet"
        );

        Ok(())
    }
}

#[tokio::test]
async fn test_volume_management() {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running");
        return;
    }

    let builder = DockerBuilder::new().unwrap();
    let volume_name = format!("test-volume-{}", uuid::Uuid::new_v4());

    // Create volume
    builder.create_volume(&volume_name).await.unwrap();

    // List volumes
    let volumes = builder.list_volumes().await.unwrap();
    assert!(volumes.contains(&volume_name));

    // Clean up
    builder.remove_volume(&volume_name).await.unwrap();
}

#[tokio::test]
async fn test_container_logs_and_exec() {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running");
        return;
    }

    let builder = DockerBuilder::new().unwrap();

    // Create a test container
    let container_config = bollard::container::Config {
        image: Some("alpine:latest"),
        cmd: Some(vec!["sh", "-c", "echo 'test message' && sleep 10"]),
        tty: Some(true),
        ..Default::default()
    };

    let container = builder
        .get_client()
        .create_container(
            Some(CreateContainerOptions {
                name: format!("test-container-{}", uuid::Uuid::new_v4()),
                platform: None,
            }),
            container_config,
        )
        .await
        .unwrap();

    builder
        .get_client()
        .start_container::<String>(&container.id, None)
        .await
        .unwrap();

    // Wait for container to be running
    builder.wait_for_container(&container.id).await.unwrap();

    // Now safe to get logs
    let logs = builder.get_container_logs(&container.id).await.unwrap();
    assert!(logs.contains("test message"));

    // Test exec
    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "test_value".to_string());

    let exec_output = builder
        .exec_in_container(&container.id, vec!["sh", "-c", "echo $TEST_VAR"], Some(env))
        .await
        .unwrap();

    assert!(exec_output.contains("test_value"));

    // Clean up
    builder
        .get_client()
        .remove_container(
            &container.id,
            Some(bollard::container::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .unwrap();
}
