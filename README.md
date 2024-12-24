# Dockworker

A Rust library for converting Dockerfile and Docker Compose configurations into programmatic container deployments using Bollard.

## Features

- **Dockerfile Parsing & Deployment**

  - Full support for all Dockerfile commands
  - Intelligent parsing of multi-line commands
  - JSON and shell syntax support
  - Resource limit configuration
  - Health check management

- **Docker Compose Support**

  - YAML configuration parsing
  - Network management with IPAM
  - Volume management
  - Service dependencies
  - Resource constraints
  - Health checks

- **Container Management**
  - Container lifecycle management
  - Log retrieval
  - Exec support
  - Network creation and cleanup
  - Volume management

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
# For parsing only (no deployment features)
dockworker = { version = "0.1.0", default-features = false, features = ["parser"] }

# For full functionality including deployment (default)
dockworker = "0.1.0"

# Or explicitly with all features
dockworker = { version = "0.1.0", features = ["docker"] }
```

## Usage

### Feature Flags

- `parser` - Enables Dockerfile and Docker Compose parsing functionality (minimal dependencies)
- `deploy` - Enables deployment features using Bollard (includes parser features)

### Parser-Only Usage

```rust
use dockworker::{DockerfileConfig, DockerfileParser, ComposeParser};

// Parse a Dockerfile
let dockerfile_content = std::fs::read_to_string("Dockerfile")?;
let config = DockerfileParser::parse(&dockerfile_content)?;

// Parse a Docker Compose file
let compose_content = std::fs::read_to_string("docker-compose.yml")?;
let compose_config = ComposeParser::parse(&compose_content)?;
```

### Full Deployment Usage

When the `deploy` feature is enabled:

```rust
use dockworker::DockerBuilder;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let builder = DockerBuilder::new()?;
    // ... deployment functionality available
    Ok(())
}
```

### Basic Dockerfile Deployment

```rust
use dockworker::{DockerBuilder, DockerfileConfig, DockerCommand};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let builder = DockerBuilder::new()?;

    // Parse existing Dockerfile
    let config = builder.from_dockerfile("path/to/Dockerfile").await?;

    // Or create programmatically
    let config = DockerfileConfig {
        base_image: "rust:1.70".to_string(),
        commands: vec![
            DockerCommand::Run {
                command: "cargo build".to_string()
            },
            DockerCommand::Copy {
                source: "./target".to_string(),
                dest: "/app".to_string(),
                chown: None,
            },
        ],
    };

    // Deploy container
    let container_id = builder.deploy_dockerfile(&config, "my-app:latest").await?;
    Ok(())
}
```

### Docker Compose Deployment

```rust
use dockworker::{DockerBuilder, ComposeConfig};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let builder = DockerBuilder::new()?;

    // Parse compose file
    let config = builder.from_compose("docker-compose.yml").await?;

    // Deploy services
    let container_ids: HashMap<String, String> = builder.deploy_compose(&config).await?;
    Ok(())
}
```

### Resource Management

```rust
use dockworker::{Service, ResourceLimits};

let service = Service {
    image: Some("nginx:latest".to_string()),
    requirements: Some(ResourceLimits {
        cpu_limit: Some(0.5),                        // Half a CPU
        memory_limit: Some("512M".to_string()),      // 512MB memory limit
        memory_swap: Some("1G".to_string()),         // 1GB swap limit
        memory_reservation: Some("256M".to_string()), // 256MB soft limit
        cpus_shares: Some(512),                      // CPU shares (relative weight)
        cpuset_cpus: Some("0,1".to_string()),        // Run on CPUs 0 and 1
    }),
    ..Default::default()
};
```

### Health Checks

```rust
use dockworker::{Service, config::compose::HealthCheck};

let service = Service {
    image: Some("nginx:latest".to_string()),
    healthcheck: Some(HealthCheck {
        test: vec![
            "CMD-SHELL".to_string(),
            "curl -f http://localhost/ || exit 1".to_string()
        ],
        interval: Some(1_000_000_000),    // 1 second
        timeout: Some(3_000_000_000),     // 3 seconds
        retries: Some(3),
        start_period: Some(2_000_000_000), // 2 seconds
        start_interval: None,
    }),
    ..Default::default()
};
```

### Container Management

```rust
use dockworker::DockerBuilder;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let builder = DockerBuilder::new()?;

    // Get container logs
    let logs = builder.get_container_logs("container_id").await?;

    // Execute command in container
    let output = builder.exec_in_container(
        "container_id",
        vec!["ls", "-la"],
        Some(HashMap::new()),
    ).await?;

    // Create and manage volumes
    builder.create_volume("my_volume").await?;
    let volumes = builder.list_volumes().await?;

    Ok(())
}
```

## Error Handling

The library provides detailed error types for proper error handling:

```rust
use dockworker::DockerError;

match result {
    Err(DockerError::FileError(e)) => println!("File error: {}", e),
    Err(DockerError::YamlError(e)) => println!("YAML parsing error: {}", e),
    Err(DockerError::DockerfileError(e)) => println!("Dockerfile parsing error: {}", e),
    Err(DockerError::BollardError(e)) => println!("Docker API error: {}", e),
    Err(DockerError::InvalidIpamConfig) => println!("Invalid network configuration"),
    Err(DockerError::ContainerNotRunning(id)) => println!("Container {} not running", id),
    Err(DockerError::NetworkCreationError(e)) => println!("Network creation failed: {}", e),
    Err(DockerError::InvalidResourceLimit(e)) => println!("Invalid resource limit: {}", e),
    Ok(_) => println!("Operation succeeded"),
}
```

## Development Status

This library is in active development. Current focus areas:

- Enhanced error handling with recovery strategies
- Comprehensive integration testing
- Additional Docker Compose features
- Improved resource management
- Better developer ergonomics

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. When contributing:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- [Bollard](https://github.com/fussybeaver/bollard) - The underlying Docker API client
- [Docker](https://www.docker.com/) - Container platform
