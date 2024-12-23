use crate::{DockerError, tests::dockerfile::is_docker_running};
use bollard::{
    container::ListContainersOptions,
    secret::{Ipam, IpamConfig},
};
use pretty_assertions::assert_eq;
use std::collections::HashMap;

use crate::{BuildConfig, ComposeConfig, DockerBuilder, ServiceConfig};

use super::fixtures::{get_local_reth_compose, get_reth_archive_compose};

#[tokio::test]
async fn test_local_reth_compose_parsing() {
    let builder = DockerBuilder::new().unwrap();
    let config = builder
        .from_compose(get_local_reth_compose())
        .await
        .unwrap();

    assert_eq!(config.version, "3.9");
    assert_eq!(config.services.len(), 3);

    // Test reth service
    let reth = config.services.get("reth").unwrap();
    assert!(reth.build.is_some());
    assert_eq!(reth.build.as_ref().unwrap().context, "./reth".to_string());
    assert_eq!(reth.ports, Some(vec!["9000:9000".to_string()]));

    // Test prometheus service
    let prometheus = config.services.get("prometheus").unwrap();
    assert_eq!(prometheus.image, Some("prom/prometheus".to_string()));
    assert_eq!(prometheus.ports, Some(vec!["9090:9090".to_string()]));

    // Test grafana service
    let grafana = config.services.get("grafana").unwrap();
    assert_eq!(grafana.image, Some("grafana/grafana".to_string()));
    assert_eq!(grafana.ports, Some(vec!["3000:3000".to_string()]));
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

    // Create a unique network name for this test
    let network_name = format!("test-network-{}", uuid::Uuid::new_v4());

    // Create network
    builder
        .get_client()
        .create_network(bollard::network::CreateNetworkOptions {
            name: network_name.as_str(),
            driver: "bridge",
            check_duplicate: true,
            internal: false,
            attachable: true,
            ingress: false,
            ipam: Ipam {
                driver: Some("default".to_string()),
                config: Some(vec![IpamConfig {
                    subnet: Some("172.30.0.0/16".to_string()),
                    gateway: Some("172.30.0.1".to_string()),
                    ip_range: None,
                    auxiliary_addresses: None,
                }]),
                options: None,
            },
            ..Default::default()
        })
        .await
        .map_err(|e| DockerError::NetworkCreationError(e.to_string()))
        .unwrap();

    // Create a simple test compose config
    let mut services = HashMap::new();
    services.insert("test-service".to_string(), ServiceConfig {
        image: Some("alpine:latest".to_string()),
        ports: Some(vec!["8080:80".to_string()]),
        environment: Some({
            let mut env = HashMap::new();
            env.insert("TEST".to_string(), "value".to_string());
            env
        }),
        volumes: None,
        networks: Some(vec![network_name.clone()]),
        ..ServiceConfig::default()
    });

    let config = ComposeConfig {
        version: "3".to_string(),
        services,
    };

    let container_ids = builder.deploy_compose(&config).await.unwrap();
    assert_eq!(container_ids.len(), 1);

    // Verify containers are running
    for (service_name, container_id) in container_ids {
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
    services.insert("build-service".to_string(), ServiceConfig {
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
    });

    let config = ComposeConfig {
        version: "3".to_string(),
        services,
    };

    let result = builder.deploy_compose(&config).await;
    // This should fail because we don't have a Dockerfile in the current directory
    assert!(result.is_err());
}
