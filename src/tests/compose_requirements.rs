use super::docker_file::is_docker_running;
use crate::DockerError;
use crate::builder::compose::parse_memory_string;
use crate::config::SystemRequirements;
use crate::{
    DockerBuilder,
    config::compose::{ComposeConfig, Service},
};
use std::collections::HashMap;

#[tokio::test]
async fn test_resource_limits() {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running");
        return;
    }

    let builder = DockerBuilder::new().unwrap();

    // Create a service with resource limits
    let mut services = HashMap::new();
    services.insert("limited-service".to_string(), Service {
        image: Some("alpine:latest".to_string()),
        requirements: Some(SystemRequirements {
            min_memory_gb: 1,
            min_disk_gb: 1,
            min_bandwidth_mbps: 100,
            required_ports: vec![],
            data_directory: "/tmp".to_string(),
            cpu_limit: Some(0.5), // Half a CPU
            memory_limit: Some("512M".to_string()),
            memory_swap: Some("1G".to_string()),
            memory_reservation: Some("256M".to_string()),
            cpu_shares: Some(512),
            cpuset_cpus: Some("0,1".to_string()),
        }),
        ..Service::default()
    });

    let mut config = ComposeConfig {
        version: "3".to_string(),
        services,
        volumes: HashMap::new(),
    };

    let container_ids = builder.deploy_compose(&mut config).await.unwrap();
    let container_id = container_ids.values().next().unwrap();

    // Verify resource limits
    let inspect = builder
        .get_client()
        .inspect_container(container_id, None)
        .await
        .unwrap();

    if let Some(host_config) = inspect.host_config {
        assert_eq!(host_config.memory, Some(512 * 1024 * 1024));
        assert_eq!(host_config.memory_swap, Some(1024 * 1024 * 1024));
        assert_eq!(host_config.cpu_shares, Some(512));
        assert_eq!(host_config.cpuset_cpus, Some("0,1".to_string()));
    }

    // Clean up
    builder
        .get_client()
        .remove_container(
            container_id,
            Some(bollard::container::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn test_invalid_resource_limits() {
    let memory_tests = vec![
        ("1X", "Invalid memory unit: X"),
        ("abc", "Invalid memory value: abc"),
        ("12.5G", "Invalid memory value: 12.5G"),
    ];

    for (input, expected_error) in memory_tests {
        let result = parse_memory_string(input);
        assert!(matches!(
            result,
            Err(DockerError::InvalidResourceLimit(msg)) if msg.contains(expected_error)
        ));
    }
}
