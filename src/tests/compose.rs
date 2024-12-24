use crate::{
    VolumeType, builder::compose::parse_memory_string, tests::docker_file::is_docker_running,
};
use bollard::container::ListContainersOptions;
use std::{collections::HashMap, path::PathBuf, time::Duration};

use crate::{BuildConfig, ComposeConfig, DockerBuilder, Service};

use super::fixtures::{get_local_reth_compose, get_reth_archive_compose};

#[test]
fn test_compose_parsing() {
    let yaml = r#"
        version: "3.8"
        services:
          reth:
            image: ghcr.io/paradigmxyz/reth:latest
            ports:
              - "8545:8545"
              - "9000:9000"
            command: [
              "/reth/target/release/reth",
              "node",
              "--metrics",
              "reth:9000",
              "--debug.tip",
              "${RETH_TIP:-0x7d5a4369273c723454ac137f48a4f142b097aa2779464e6505f1b1c5e37b5382}",
              "--log.directory",
              "$HOME"
            ]
            volumes:
              - source: ./data
                target: /data
                type: bind
                read_only: false
    "#;

    let config: ComposeConfig = serde_yaml::from_str(yaml).unwrap();
    let service = config.services.get("reth").unwrap();

    assert!(service.command.is_some());
    let command = service.command.as_ref().unwrap();
    assert_eq!(command.len(), 8);
    assert_eq!(command[0], "/reth/target/release/reth");
    assert_eq!(command[1], "node");
    // Test volume configuration
    let volumes = service.volumes.as_ref().unwrap();
    assert_eq!(volumes.len(), 1);

    match &volumes[0] {
        VolumeType::Bind {
            source,
            target,
            read_only,
        } => {
            assert_eq!(source, "./data");
            assert_eq!(target, "/data");
            assert!(!read_only);
        }
        _ => panic!("Expected bind mount"),
    }
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

    // Test volumes are defined
    assert!(config.volumes.contains_key("reth_data"));
    assert!(config.volumes.contains_key("reth_jwt"));
    assert!(config.volumes.contains_key("nimbus_data"));

    // Test reth service volumes
    let reth_volumes = reth.volumes.as_ref().unwrap();
    assert_eq!(reth_volumes.len(), 2);
    assert!(matches!(&reth_volumes[0], VolumeType::Named(name) if name == "reth_data:/data"));
    assert!(matches!(&reth_volumes[1], VolumeType::Named(name) if name == "reth_jwt:/jwt:ro"));

    // Test nimbus service volumes
    let nimbus_volumes = nimbus.volumes.as_ref().unwrap();
    assert_eq!(nimbus_volumes.len(), 2);
    assert!(matches!(&nimbus_volumes[0], VolumeType::Named(name) if name == "nimbus_data:/data"));
    assert!(
        matches!(&nimbus_volumes[1], VolumeType::Named(name) if name == "reth_jwt:/jwt/reth:ro")
    );
}

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
    assert_eq!(reth.image, None); // Local build, no image specified
    assert_eq!(reth.ports, Some(vec!["9000:9000".to_string()]));
    assert!(reth.build.is_some());
    let build = reth.build.as_ref().unwrap();
    assert_eq!(build.context, "./reth");
    assert_eq!(build.dockerfile, Some("Dockerfile".to_string()));
    assert!(reth.command.is_some());
    assert!(reth.volumes.is_some());
    assert_eq!(reth.restart, Some("always".to_string()));

    // Test prometheus service
    let prometheus = config.services.get("prometheus").unwrap();
    assert_eq!(prometheus.image, Some("prom/prometheus".to_string()));
    assert_eq!(prometheus.ports, Some(vec!["9090:9090".to_string()]));
    assert!(prometheus.volumes.is_some());
    assert_eq!(prometheus.restart, Some("always".to_string()));
    assert_eq!(prometheus.user, Some("root".to_string()));
    assert!(prometheus.depends_on.is_some());
    assert!(prometheus.command.is_some());

    // Test grafana service
    let grafana = config.services.get("grafana").unwrap();
    assert_eq!(grafana.image, Some("grafana/grafana".to_string()));
    assert_eq!(grafana.ports, Some(vec!["3000:3000".to_string()]));
    assert!(grafana.volumes.is_some());
    assert_eq!(grafana.restart, Some("always".to_string()));
    assert_eq!(grafana.user, Some("472".to_string()));
    assert!(grafana.depends_on.is_some());

    // Test volumes are defined
    assert!(config.volumes.contains_key("rethdata"));
    assert!(config.volumes.contains_key("rethlogs"));
    assert!(config.volumes.contains_key("prometheusdata"));
    assert!(config.volumes.contains_key("grafanadata"));
    // Test volume configurations
    for volume_name in ["rethdata", "rethlogs", "prometheusdata", "grafanadata"] {
        let volume = config.volumes.get(volume_name).unwrap();
        match volume {
            VolumeType::Config { driver, .. } => {
                assert_eq!(driver.as_deref(), Some("local"));
            }
            _ => panic!("Expected Config volume type for {}", volume_name),
        }
    }

    // Test volume mount paths for reth service
    let reth_volumes = reth.volumes.as_ref().unwrap();
    assert!(reth_volumes.iter().any(|v| match v {
        VolumeType::Named(s) => s.starts_with("rethdata:"),
        _ => false,
    }));
    assert!(reth_volumes.iter().any(|v| match v {
        VolumeType::Named(s) => s.starts_with("rethlogs:"),
        _ => false,
    }));

    // Test volume mount paths for prometheus service
    let prom_volumes = prometheus.volumes.as_ref().unwrap();
    assert!(prom_volumes.iter().any(|v| match v {
        VolumeType::Bind { source, target, .. } => {
            source == "./prometheus/" && target == "/etc/prometheus/"
        }
        _ => false,
    }));
    assert!(prom_volumes.iter().any(|v| match v {
        VolumeType::Named(s) => s.starts_with("prometheusdata:"),
        _ => false,
    }));

    // Test volume mount paths for grafana service
    let grafana_volumes = grafana.volumes.as_ref().unwrap();
    assert!(grafana_volumes.iter().any(|v| match v {
        VolumeType::Named(s) => s.starts_with("grafanadata:"),
        _ => false,
    }));
    assert!(grafana_volumes.iter().any(|v| match v {
        VolumeType::Bind { source, target, .. } => {
            source == "./grafana/provisioning/" && target == "/etc/grafana/provisioning/"
        }
        _ => false,
    }));
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

    let mut config = ComposeConfig {
        version: "3".to_string(),
        services,
        volumes: HashMap::new(),
    };

    let container_ids = builder.deploy_compose(&mut config).await.unwrap();
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
        requirements: None,
        depends_on: None,
        healthcheck: None,
        restart: None,
        command: None,
        user: None,
    });

    let mut config = ComposeConfig {
        version: "3".to_string(),
        services,
        volumes: HashMap::new(),
    };

    let result = builder.deploy_compose(&mut config).await;
    // This should fail because we don't have a Dockerfile in the current directory
    assert!(result.is_err());
}

#[test]
fn test_compose_volume_parsing() {
    let yaml = r#"
            version: "3.8"
            services:
              test:
                volumes:
                  - source: named_vol
                    target: /data
                    type: volume
                  - source: ./local
                    target: /container
                    type: bind
                  - source: /abs/path
                    target: /container/config
                    type: bind
                    read_only: true
        "#;

    let config: ComposeConfig = serde_yaml::from_str(yaml).unwrap();
    let service = config.services.get("test").unwrap();
    let volumes = service.volumes.as_ref().unwrap();

    assert_eq!(volumes.len(), 3);

    // Check named volume
    assert!(matches!(&volumes[0], VolumeType::Named(name) if name == "named_vol:/data"));

    // Check relative path bind mount
    match &volumes[1] {
        VolumeType::Bind {
            source,
            target,
            read_only,
        } => {
            assert_eq!(
                source,
                &PathBuf::from("./local").to_string_lossy().to_string()
            );
            assert_eq!(target, "/container");
            assert!(!read_only);
        }
        _ => panic!("Expected bind mount"),
    }

    // Check absolute path bind mount with read-only
    match &volumes[2] {
        VolumeType::Bind {
            source,
            target,
            read_only,
        } => {
            assert_eq!(
                source,
                &PathBuf::from("/abs/path").to_string_lossy().to_string()
            );
            assert_eq!(target, "/container/config");
            assert!(*read_only);
        }
        _ => panic!("Expected bind mount"),
    }
}

#[test]
fn test_service_volume_fixing() {
    let mut config = ComposeConfig::default();
    let mut service = Service::default();
    service.volumes = Some(vec![VolumeType::Bind {
        source: "./data".to_string(),
        target: "/container/data".to_string(),
        read_only: false,
    }]);
    config.services.insert("test_service".to_string(), service);

    let volumes = config.services["test_service"].volumes.as_ref().unwrap();
    assert!(
        matches!(&volumes[0], VolumeType::Bind { source, target, read_only }
            if source == "./data"
                && target == "/container/data"
                && !read_only
        )
    );
}

#[test]
fn test_volume_serialization() {
    let volume = VolumeType::Bind {
        source: PathBuf::from("/host").to_string_lossy().to_string(),
        target: "/container".to_string(),
        read_only: true,
    };

    let service = Service {
        volumes: Some(vec![volume]),
        ..Default::default()
    };

    let serialized = serde_yaml::to_string(&service).unwrap();
    let deserialized: Service = serde_yaml::from_str(&serialized).unwrap();

    assert_eq!(service.volumes, deserialized.volumes);
}

#[test]
fn test_volume_validation() {
    let mut config = ComposeConfig {
        version: "3.8".to_string(),
        services: HashMap::new(),
        volumes: HashMap::new(),
    };

    let service = Service {
        volumes: Some(vec![
            VolumeType::Named("data:/data".to_string()),
            VolumeType::Bind {
                source: PathBuf::from("/host").to_string_lossy().to_string(),
                target: "/container".to_string(),
                read_only: false,
            },
        ]),
        ..Default::default()
    };

    config.services.insert("test".to_string(), service);

    // Test validation of named volumes
    assert!(config.validate_required_volumes(&["data"]).is_ok());

    // Test validation of bind mounts by target path
    assert!(config.validate_required_volumes(&["/container"]).is_ok());

    // Test validation of missing volumes
    assert!(config.validate_required_volumes(&["missing"]).is_err());

    // Test validation of missing bind mount targets
    assert!(config.validate_required_volumes(&["/missing"]).is_err());
}

#[test]
fn test_service_deployment() {
    let mut config = ComposeConfig::default();
    let service = Service {
        image: Some("nginx:latest".to_string()),
        volumes: Some(vec![crate::VolumeType::Bind {
            source: PathBuf::from("/host/data").to_string_lossy().to_string(),
            target: "/container/data".to_string(),
            read_only: false,
        }]),
        ..Default::default()
    };

    config.services.insert("web".to_string(), service);

    // Add test assertions here
    assert!(config.services.contains_key("web"));
}

#[test]
fn test_memory_string_parsing() {
    assert_eq!(parse_memory_string("512M").unwrap(), 512 * 1024 * 1024);
    assert_eq!(parse_memory_string("1G").unwrap(), 1024 * 1024 * 1024);
    assert!(parse_memory_string("invalid").is_err());
}
