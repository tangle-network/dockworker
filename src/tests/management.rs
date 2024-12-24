use crate::DockerBuilder;
use std::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn test_network_management() -> Result<(), Box<dyn std::error::Error>> {
    let builder = DockerBuilder::new()?;
    let network_name = format!("test-network-{}", Uuid::new_v4());

    // Create network with retry
    builder
        .create_network_with_retry(&network_name, 5, Duration::from_millis(100))
        .await?;

    // Verify network exists
    let networks = builder.list_networks().await?;
    assert!(
        networks.contains(&network_name),
        "Created network should be in the list"
    );

    // Clean up
    builder.remove_network(&network_name).await?;

    // Verify removal
    let networks_after = builder.list_networks().await?;
    assert!(
        !networks_after.contains(&network_name),
        "Removed network should not be in the list"
    );

    Ok(())
}

#[tokio::test]
async fn test_volume_management() -> Result<(), Box<dyn std::error::Error>> {
    let builder = DockerBuilder::new()?;
    let volume_name = format!("test-volume-{}", Uuid::new_v4());

    // Create volume
    builder.create_volume(&volume_name).await?;

    // Verify volume exists
    let volumes = builder.list_volumes().await?;
    assert!(
        volumes.contains(&volume_name),
        "Created volume should be in the list"
    );

    // Clean up
    builder.remove_volume(&volume_name).await?;

    // Verify removal
    let volumes_after = builder.list_volumes().await?;
    assert!(
        !volumes_after.contains(&volume_name),
        "Removed volume should not be in the list"
    );

    Ok(())
}

#[tokio::test]
async fn test_container_management() -> Result<(), Box<dyn std::error::Error>> {
    let builder = DockerBuilder::new()?;
    let container_name = format!("test-container-{}", Uuid::new_v4());

    // Create container
    let container = builder
        .get_client()
        .create_container(
            Some(bollard::container::CreateContainerOptions {
                name: container_name.clone(),
                platform: None,
            }),
            bollard::container::Config {
                image: Some("alpine:latest".to_string()),
                cmd: Some(vec![
                    "sh".to_string(),
                    "-c".to_string(),
                    "echo test message && sleep 1".to_string(),
                ]),
                ..Default::default()
            },
        )
        .await?;

    // Start container
    builder
        .get_client()
        .start_container(
            &container.id,
            None::<bollard::container::StartContainerOptions<String>>,
        )
        .await?;

    // Wait for container to be running
    builder.wait_for_container(&container.id).await?;

    // Get logs
    let logs = builder.get_container_logs(&container.id).await?;
    assert!(logs.contains("test message"));

    // Test exec
    let exec_output = builder
        .exec_in_container(&container.id, vec!["echo", "exec test"], None)
        .await?;
    assert!(exec_output.contains("exec test"));

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
        .await?;

    Ok(())
}
