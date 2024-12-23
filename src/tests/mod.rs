mod compose;
mod dockerfile;
mod fixtures;

use std::path::PathBuf;

pub(crate) fn fixtures_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}
