use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerfileConfig {
    pub(crate) base_image: String,
    pub(crate) commands: Vec<DockerCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum DockerCommand {
    Add {
        sources: Vec<String>,
        dest: String,
        chown: Option<String>,
    },
    Arg {
        name: String,
        default_value: Option<String>,
    },
    Cmd {
        command: Vec<String>,
    },
    Copy {
        source: String,
        dest: String,
        chown: Option<String>,
    },
    Entrypoint {
        command: Vec<String>,
    },
    Env {
        key: String,
        value: String,
    },
    Expose {
        port: u16,
        protocol: Option<String>,
    },
    Healthcheck {
        command: Vec<String>,
        interval: Option<String>,
        timeout: Option<String>,
        start_period: Option<String>,
        retries: Option<u32>,
    },
    Label {
        labels: HashMap<String, String>,
    },
    Maintainer {
        name: String,
    },
    Onbuild {
        command: Box<DockerCommand>,
    },
    Run {
        command: String,
    },
    Shell {
        shell: Vec<String>,
    },
    StopSignal {
        signal: String,
    },
    User {
        user: String,
        group: Option<String>,
    },
    Volume {
        paths: Vec<String>,
    },
    Workdir {
        path: String,
    },
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
            DockerCommand::Add {
                sources,
                dest,
                chown,
            } => {
                if let Some(chown) = chown {
                    format!("ADD --chown={} {} {}", chown, sources.join(" "), dest)
                } else {
                    format!("ADD {} {}", sources.join(" "), dest)
                }
            }
            DockerCommand::Arg {
                name,
                default_value,
            } => {
                if let Some(value) = default_value {
                    format!("ARG {}={}", name, value)
                } else {
                    format!("ARG {}", name)
                }
            }
            DockerCommand::Cmd { command } => format!("CMD {}", shell_words::join(command)),
            DockerCommand::Copy {
                source,
                dest,
                chown,
            } => {
                if let Some(chown) = chown {
                    format!("COPY --chown={} {} {}", chown, source, dest)
                } else {
                    format!("COPY {} {}", source, dest)
                }
            }
            DockerCommand::Entrypoint { command } => {
                format!("ENTRYPOINT {}", shell_words::join(command))
            }
            DockerCommand::Env { key, value } => format!("ENV {}={}", key, value),
            DockerCommand::Expose { port, protocol } => {
                if let Some(proto) = protocol {
                    format!("EXPOSE {}/{}", port, proto)
                } else {
                    format!("EXPOSE {}", port)
                }
            }
            DockerCommand::Healthcheck {
                command,
                interval,
                timeout,
                start_period,
                retries,
            } => {
                let mut options = Vec::new();
                if let Some(interval) = interval {
                    options.push(format!("--interval={}", interval));
                }
                if let Some(timeout) = timeout {
                    options.push(format!("--timeout={}", timeout));
                }
                if let Some(start_period) = start_period {
                    options.push(format!("--start-period={}", start_period));
                }
                if let Some(retries) = retries {
                    options.push(format!("--retries={}", retries));
                }
                let options_str = if options.is_empty() {
                    String::new()
                } else {
                    format!(" {}", options.join(" "))
                };
                format!(
                    "HEALTHCHECK{}  CMD {}",
                    options_str,
                    shell_words::join(command)
                )
            }
            DockerCommand::Label { labels } => {
                let labels = labels
                    .iter()
                    .map(|(k, v)| format!("{}=\"{}\"", k, v))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("LABEL {}", labels)
            }
            DockerCommand::Maintainer { name } => format!("MAINTAINER {}", name),
            DockerCommand::Onbuild { command } => format!("ONBUILD {}", command.to_string()),
            DockerCommand::Run { command } => format!("RUN {}", command),
            DockerCommand::Shell { shell } => format!("SHELL {}", shell_words::join(shell)),
            DockerCommand::StopSignal { signal } => format!("STOPSIGNAL {}", signal),
            DockerCommand::User { user, group } => {
                if let Some(group) = group {
                    format!("USER {}:{}", user, group)
                } else {
                    format!("USER {}", user)
                }
            }
            DockerCommand::Volume { paths } => format!("VOLUME {}", shell_words::join(paths)),
            DockerCommand::Workdir { path } => format!("WORKDIR {}", path),
        }
    }
}
