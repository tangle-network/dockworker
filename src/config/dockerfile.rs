use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerfileConfig {
    pub(crate) base_image: String,
    pub(crate) commands: Vec<DockerCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DockerCommand {
    Run { command: String },
    Copy { source: String, dest: String },
    Env { key: String, value: String },
    Workdir { path: String },
    Expose { port: u16 },
}

impl DockerfileConfig {
    pub fn to_dockerfile_content(&self) -> String {
        let mut content = format!("FROM {}\n", self.base_image);

        for command in &self.commands {
            content.push_str(&format!("{}\n", command.to_string()));
        }

        content
    }
}

impl ToString for DockerCommand {
    fn to_string(&self) -> String {
        match self {
            DockerCommand::Run { command } => format!("RUN {}", command),
            DockerCommand::Copy { source, dest } => format!("COPY {} {}", source, dest),
            DockerCommand::Env { key, value } => format!("ENV {}={}", key, value),
            DockerCommand::Workdir { path } => format!("WORKDIR {}", path),
            DockerCommand::Expose { port } => format!("EXPOSE {}", port),
        }
    }
}
