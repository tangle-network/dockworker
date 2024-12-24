use bollard::Docker;
use bollard::container::ListContainersOptions;
use bollard::network::ListNetworksOptions;
use bollard::volume::ListVolumesOptions;
use std::collections::HashMap;
use uuid::Uuid;

pub struct DockerTestContext {
    client: Docker,
    test_id: String,
}

impl DockerTestContext {
    pub fn new() -> Self {
        let client = Docker::connect_with_local_defaults().unwrap();
        let test_id = Uuid::new_v4().to_string();
        Self { client, test_id }
    }

    pub fn get_test_id(&self) -> &str {
        &self.test_id
    }

    pub async fn cleanup(&self) {
        // Clean up containers
        let mut filters = HashMap::new();
        filters.insert(String::from("label"), vec![format!(
            "test_id={}",
            self.test_id
        )]);

        if let Ok(containers) = self
            .client
            .list_containers(Some(ListContainersOptions {
                all: true,
                filters,
                ..Default::default()
            }))
            .await
        {
            for container in containers {
                if let Some(id) = container.id {
                    let _ = self
                        .client
                        .remove_container(
                            &id,
                            Some(bollard::container::RemoveContainerOptions {
                                force: true,
                                ..Default::default()
                            }),
                        )
                        .await;
                }
            }
        }

        // Clean up networks
        let mut filters = HashMap::new();
        filters.insert(String::from("label"), vec![format!(
            "test_id={}",
            self.test_id
        )]);

        if let Ok(networks) = self
            .client
            .list_networks(Some(ListNetworksOptions { filters }))
            .await
        {
            for network in networks {
                if let Some(id) = network.id {
                    let _ = self.client.remove_network(&id).await;
                }
            }
        }

        // Clean up volumes
        let mut filters = HashMap::new();
        filters.insert(String::from("label"), vec![format!(
            "test_id={}",
            self.test_id
        )]);

        if let Ok(volumes) = self
            .client
            .list_volumes(Some(ListVolumesOptions { filters }))
            .await
        {
            if let Some(volume_list) = volumes.volumes {
                for volume in volume_list {
                    let _ = self.client.remove_volume(&volume.name, None).await;
                }
            }
        }
    }
}

/// Helper macro to create a test context and ensure cleanup
#[macro_export]
macro_rules! with_docker_cleanup {
    ($test_name:ident, $test_body:expr) => {
        #[tokio::test]
        async fn $test_name() -> Result<(), Box<dyn std::error::Error>> {
            let ctx = $crate::tests::utils::DockerTestContext::new();
            // Clean up any leftover resources from previous test runs
            ctx.cleanup().await;

            // Run the test with the context
            let test_id = ctx.get_test_id().to_string();
            ($test_body)(&test_id).await;

            // Clean up after the test
            ctx.cleanup().await;

            Ok(())
        }
    };
}
