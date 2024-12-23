// Parser tests always available
mod compose;
mod docker_file;

// Deployment tests only with deploy feature
mod compose_health;
mod compose_resources;
mod management;

mod fixtures;

use std::path::PathBuf;

pub(crate) fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}
