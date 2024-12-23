use std::path::PathBuf;

use super::fixtures_path;

pub fn get_tangle_dockerfile() -> PathBuf {
    fixtures_path().join("tangle-dockerfile")
}

pub fn get_local_reth_compose() -> PathBuf {
    fixtures_path().join("local-reth-docker-compose.yml")
}

pub fn get_reth_archive_compose() -> PathBuf {
    fixtures_path().join("reth-archive-docker-compose.yml")
}
