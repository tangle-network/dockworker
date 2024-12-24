use super::docker_file::is_docker_running;
use crate::{
    DockerBuilder,
    config::compose::{ComposeConfig, HealthCheck, Service},
};
use futures_util::TryStreamExt;
use std::collections::HashMap;

#[tokio::test]
async fn test_healthcheck() {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running");
        return;
    }

    let builder = DockerBuilder::new().unwrap();

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
    services.insert("healthy-service".to_string(), Service {
        image: Some("nginx:latest".to_string()),
        healthcheck: Some(HealthCheck {
            test: vec![
                "CMD-SHELL".to_string(),
                "curl -f http://localhost/ || exit 1".to_string(),
            ],
            interval: Some(1_000_000_000), // 1 second
            timeout: Some(3_000_000_000),  // 3 seconds
            retries: Some(3),
            start_period: Some(2_000_000_000), // 2 seconds
            start_interval: None,
        }),
        ports: Some(vec!["8080:80".to_string()]),
        ..Default::default()
    });

    let config = ComposeConfig {
        version: "3".to_string(),
        services,
    };

    let container_ids = builder.deploy_compose(&config).await.unwrap();
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
                    "curl -f http://localhost/ || exit 1".to_string()
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
