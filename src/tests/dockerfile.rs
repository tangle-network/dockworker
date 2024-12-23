use bollard::container::ListContainersOptions;
use pretty_assertions::assert_eq;
use std::time::Duration;

use crate::{DockerBuilder, DockerCommand, DockerfileConfig};

use super::fixtures::get_tangle_dockerfile;

#[tokio::test]
async fn test_dockerfile_parsing() {
    let builder = DockerBuilder::new().unwrap();
    let config = builder
        .from_dockerfile(get_tangle_dockerfile())
        .await
        .unwrap();

    assert_eq!(config.base_image, "ubuntu:22.04");
    assert_eq!(config.commands.len(), 3);

    let commands = config.commands;
    match &commands[0] {
        DockerCommand::Copy { source, dest } => {
            assert_eq!(source, "./target/release/tangle");
            assert_eq!(dest, "/usr/local/bin/");
        }
        _ => panic!("Expected COPY command"),
    }

    // Verify RUN command with multiple operations
    match &commands[1] {
        DockerCommand::Run { command } => {
            let cmd = command.to_string();
            assert!(cmd.contains("useradd"));
            assert!(cmd.contains("mkdir"));
            assert!(cmd.contains("chown"));
            assert!(cmd.contains("/usr/local/bin/tangle --version"));
        }
        _ => panic!("Expected RUN command"),
    }
}

#[tokio::test]
async fn test_dockerfile_deployment() {
    let builder = DockerBuilder::new().unwrap();

    // Create a simple test Dockerfile config
    let config = DockerfileConfig {
        base_image: "alpine:latest".to_string(),
        commands: vec![DockerCommand::Run {
            command: "echo test".to_string(),
        }],
    };

    let tag = format!("test-dockerfile:{}", uuid::Uuid::new_v4());
    let container_id = builder.deploy_dockerfile(&config, &tag).await.unwrap();

    // Verify container is running
    let mut filters = std::collections::HashMap::new();
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

    // Clean up
    tokio::time::sleep(Duration::from_secs(1)).await;
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

    // Clean up the image
    builder
        .get_client()
        .remove_image(&tag, None, None)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_dockerfile_content_generation() {
    let config = DockerfileConfig {
        base_image: "rust:1.70".to_string(),
        commands: vec![
            DockerCommand::Run {
                command: "cargo build".to_string(),
            },
            DockerCommand::Copy {
                source: "./target".to_string(),
                dest: "/app".to_string(),
            },
            DockerCommand::Env {
                key: "RUST_LOG".to_string(),
                value: "debug".to_string(),
            },
            DockerCommand::Workdir {
                path: "/app".to_string(),
            },
            DockerCommand::Expose { port: 8080 },
        ],
    };

    let content = config.to_dockerfile_content();
    let expected = r#"FROM rust:1.70
RUN cargo build
COPY ./target /app
ENV RUST_LOG=debug
WORKDIR /app
EXPOSE 8080
"#;

    assert_eq!(content, expected);
}
