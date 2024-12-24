// Parser tests always available
mod compose;
mod docker_file;

// Deployment tests only with deploy feature
mod compose_health;
mod compose_requirements;
mod fixtures;
mod integration;
mod management;

use std::path::PathBuf;

pub(crate) fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}
