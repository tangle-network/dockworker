mod compose;
mod compose_health;
mod compose_resources;
mod dockerfile;
mod fixtures;
mod management;

use std::path::PathBuf;

pub(crate) fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}
