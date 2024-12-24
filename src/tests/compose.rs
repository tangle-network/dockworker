use crate::tests::docker_file::is_docker_running;
use bollard::container::ListContainersOptions;
use std::{collections::HashMap, time::Duration};

use crate::{BuildConfig, ComposeConfig, DockerBuilder, Service};

use super::fixtures::{get_local_reth_compose, get_reth_archive_compose};

#[test]
fn test_local_reth_compose_parsing() {
    let yaml = r#"
        version: "3.8"
        services:
          reth:
            image: ghcr.io/paradigmxyz/reth:latest
            ports:
              - "8545:8545"
              - "9000:9000"
            command:
              - /reth/target/release/reth
              - node
              - --metrics
              - reth:9000
              - --debug.tip
              - ${RETH_TIP:-0x7d5a4369273c723454ac137f48a4f142b097aa2779464e6505f1b1c5e37b5382}
              - --log.directory
              - $HOME
            volumes:
              - type: bind
                source: ./data
                target: /data
                read_only: false
    "#;

    let config: ComposeConfig = serde_yaml::from_str(yaml).unwrap();
    let service = config.services.get("reth").unwrap();

    assert!(service.command.is_some());
    let command = service.command.as_ref().unwrap();
    assert_eq!(command.len(), 8);
    assert_eq!(command[0], "/reth/target/release/reth");
    assert_eq!(command[1], "node");
}

#[tokio::test]
async fn test_reth_archive_compose_parsing() {
    let builder = DockerBuilder::new().unwrap();
    let config = builder
        .from_compose(get_reth_archive_compose())
        .await
        .unwrap();

    assert_eq!(config.version, "2");
    assert_eq!(config.services.len(), 2);

    // Test reth service
    let reth = config.services.get("reth").unwrap();
    assert_eq!(
        reth.image,
        Some("ghcr.io/paradigmxyz/reth:latest".to_string())
    );
    assert_eq!(
        reth.ports,
        Some(vec![
            "8543:8543".to_string(),
            "8544:8544".to_string(),
            "30304:30304/tcp".to_string(),
            "30304:30304/udp".to_string(),
        ])
    );

    // Test nimbus service
    let nimbus = config.services.get("nimbus").unwrap();
    assert_eq!(
        nimbus.image,
        Some("statusim/nimbus-eth2:multiarch-latest".to_string())
    );
    assert_eq!(
        nimbus.ports,
        Some(vec![
            "9001:9001/tcp".to_string(),
            "9001:9001/udp".to_string(),
        ])
    );
}

#[tokio::test]
async fn test_compose_deployment() {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running");
        return;
    }

    let builder = DockerBuilder::new().unwrap();
    let network_name = format!("test-network-{}", uuid::Uuid::new_v4());

    // Create network with retry mechanism
    builder
        .create_network_with_retry(&network_name, 3, Duration::from_secs(2))
        .await
        .unwrap();

    // Create a simple test compose config
    let mut services = HashMap::new();
    services.insert("test-service".to_string(), Service {
        image: Some("alpine:latest".to_string()),
        ports: Some(vec!["8080:80".to_string()]),
        environment: Some({
            let mut env = HashMap::new();
            env.insert("TEST".to_string(), "value".to_string());
            env
        }),
        volumes: None,
        networks: Some(vec![network_name.clone()]),
        ..Service::default()
    });

    let config = ComposeConfig {
        version: "3".to_string(),
        services,
    };

    let container_ids = builder.deploy_compose(&config).await.unwrap();
    assert_eq!(container_ids.len(), 1);

    // Verify containers are running
    for (_, container_id) in container_ids {
        let mut filters = HashMap::new();
        filters.insert("id".to_string(), vec![container_id.clone()]);

        let containers = builder
            .get_client()
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await
            .unwrap();

        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].id.as_ref().unwrap(), &container_id);

        // Clean up container
        builder
            .get_client()
            .remove_container(
                &container_id,
                Some(bollard::container::RemoveContainerOptions {
                    force: true,
                    ..Default::default()
                }),
            )
            .await
            .unwrap();
    }

    // Clean up network
    builder
        .get_client()
        .remove_network(&network_name)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_compose_with_build() {
    let builder = DockerBuilder::new().unwrap();

    // Create a compose config with build context
    let mut services = HashMap::new();
    services.insert("build-service".to_string(), Service {
        image: None,
        build: Some(BuildConfig {
            context: "./".to_string(),
            dockerfile: Some("Dockerfile".to_string()),
        }),
        ports: None,
        environment: None,
        volumes: None,
        networks: None,
        resources: None,
        depends_on: None,
        healthcheck: None,
        restart: None,
        command: None,
    });

    let config = ComposeConfig {
        version: "3".to_string(),
        services,
    };

    let result = builder.deploy_compose(&config).await;
    // This should fail because we don't have a Dockerfile in the current directory
    assert!(result.is_err());
}
