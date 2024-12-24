// Parser tests always available
pub mod compose;
pub mod docker_file;
pub mod fixtures;
pub mod utils;

// Deployment tests only with deploy feature
mod compose_health;
mod compose_requirements;
mod integration;
mod management;

use std::path::PathBuf;

pub(crate) fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}
