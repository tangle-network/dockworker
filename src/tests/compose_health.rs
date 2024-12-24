use super::docker_file::is_docker_running;
use crate::{
    DockerBuilder,
    config::{
        HealthCheck,
        compose::{ComposeConfig, Service},
    },
};
use futures_util::TryStreamExt;
use std::{collections::HashMap, time::Duration};

#[tokio::test]
async fn test_healthcheck() {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running");
        return;
    }

    let builder = DockerBuilder::new().unwrap();
    let service_name = "healthy-service";

    // Clean up any existing container with the same name
    let _ = builder
        .get_client()
        .remove_container(
            service_name,
            Some(bollard::container::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;

    // Pull nginx image first
    builder
        .get_client()
        .create_image(
            Some(bollard::image::CreateImageOptions {
                from_image: "nginx",
                tag: "latest",
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await
        .unwrap();

    // Create a service with healthcheck
    let mut services = HashMap::new();
    services.insert(service_name.to_string(), Service {
        image: Some("nginx:latest".to_string()),
        healthcheck: Some(HealthCheck {
            endpoint: "http://localhost/".to_string(),
            method: "GET".to_string(),
            expected_status: 200,
            body: None,
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(3),
            retries: 3,
        }),
        ports: Some(vec!["8080:80".to_string()]),
        ..Default::default()
    });

    let mut config = ComposeConfig {
        version: "3".to_string(),
        services,
        volumes: HashMap::new(),
    };

    let container_ids = builder.deploy_compose(&mut config).await.unwrap();
    let container_id = container_ids.values().next().unwrap();

    // Wait for container to be healthy
    builder.wait_for_container(&container_id).await.unwrap();

    // Verify healthcheck configuration
    let inspect = builder
        .get_client()
        .inspect_container(container_id, None)
        .await
        .unwrap();

    if let Some(config) = inspect.config {
        if let Some(healthcheck) = config.healthcheck {
            assert_eq!(
                healthcheck.test,
                Some(vec![
                    "CMD-SHELL".to_string(),
                    format!(
                        "curl -X GET {} -s -f -o /dev/null -w '%{{http_code}}' | grep -q {}",
                        "http://localhost/", "200"
                    )
                ])
            );
            assert_eq!(healthcheck.interval, Some(1_000_000_000));
            assert_eq!(healthcheck.timeout, Some(3_000_000_000));
            assert_eq!(healthcheck.retries, Some(3));
        }
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
