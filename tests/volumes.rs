mod common;

use common::with_docker_cleanup;
use dockworker::DockerBuilder;

#[tokio::test]
async fn test_volume_management() -> color_eyre::Result<()> {
    with_docker_cleanup(|test_id| {
        Box::pin(async move {
            let builder = DockerBuilder::new().await?;
            let volume_name = format!("test-volume-{}", test_id);

            // Create volume
            builder.create_volume(&volume_name).await?;

            // Verify volume exists
            let volumes = builder.list_volumes().await?;
            assert!(
                volumes.contains(&volume_name),
                "Created volume should be in the list"
            );

            Ok(())
        })
    })
    .await
}
