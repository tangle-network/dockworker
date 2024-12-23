use bollard::container::{CreateContainerOptions, StartContainerOptions};

use super::dockerfile::is_docker_running;
use crate::DockerBuilder;
use std::collections::HashMap;

#[tokio::test]
async fn test_network_management() {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running");
        return;
    }

    let builder = DockerBuilder::new().unwrap();
    let network_name = format!("test-network-{}", uuid::Uuid::new_v4());

    // Create network
    builder
        .create_network(&network_name, Some("172.20.0.0/16"), Some("172.20.0.1"))
        .await
        .unwrap();

    // List networks
    let networks = builder.list_networks().await.unwrap();
    assert!(networks.contains(&network_name));

    // Clean up
    builder.remove_network(&network_name).await.unwrap();
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
