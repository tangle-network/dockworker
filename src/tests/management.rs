use bollard::container::CreateContainerOptions;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;

use super::docker_file::is_docker_running;
use crate::DockerBuilder;

#[tokio::test]
async fn test_network_management() {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running");
        return;
    }

    let builder = DockerBuilder::new().unwrap();
    let test_network = format!("test-network-{}", uuid::Uuid::new_v4());

    // Test 1: Network Creation
    let result = timeout(
        Duration::from_secs(5),
        builder.create_network(
            &test_network,
            "172.18.0.0/16", // Use a specific subnet for testing
            "172.18.0.1",
        ),
    )
    .await
    .expect("Network creation timed out")
    .expect("Failed to create network");

    assert!(result.id.is_some(), "Network creation should return an ID");

    // Test 2: Network Listing
    let networks = timeout(Duration::from_secs(5), builder.list_networks())
        .await
        .expect("Network listing timed out")
        .expect("Failed to list networks");

    assert!(
        networks.contains(&test_network),
        "Created network should be in the list"
    );

    // Test 3: Network Removal
    timeout(
        Duration::from_secs(5),
        builder.remove_network(&test_network),
    )
    .await
    .expect("Network removal timed out")
    .expect("Failed to remove network");

    // Verify removal
    let networks_after = timeout(Duration::from_secs(5), builder.list_networks())
        .await
        .expect("Network listing timed out")
        .expect("Failed to list networks");

    assert!(
        !networks_after.contains(&test_network),
        "Removed network should not be in the list"
    );
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
