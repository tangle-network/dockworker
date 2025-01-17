cfg_if::cfg_if! {
    // Parser tests always available
    if #[cfg(test)] {
        pub mod compose;
        pub mod docker_file;
        pub mod fixtures;

        // Deployment tests only with deploy feature
        mod compose_health;
        mod compose_requirements;
        mod management;
    }
}

// Utils are always available for integration tests
pub mod utils;
