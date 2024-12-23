use crate::{
    config::dockerfile::{DockerCommand, DockerfileConfig},
    error::DockerError,
};

pub struct DockerfileParser;

impl DockerfileParser {
    pub fn parse(content: &str) -> Result<DockerfileConfig, DockerError> {
        let mut config = DockerfileConfig {
            base_image: String::new(),
            commands: Vec::new(),
        };

        let mut current_command = String::new();
        let mut lines = content.lines().peekable();

        while let Some(line) = lines.next() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Handle line continuations
            if line.ends_with('\\') {
                current_command.push_str(&line[..line.len() - 1]);
                current_command.push(' ');
                continue;
            } else if !current_command.is_empty() {
                current_command.push_str(line);
                Self::parse_command(&mut config, &current_command)?;
                current_command.clear();
                continue;
            }

            Self::parse_command(&mut config, line)?;
        }

        Ok(config)
    }

    fn parse_command(config: &mut DockerfileConfig, line: &str) -> Result<(), DockerError> {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Ok(());
        }

        match parts[0].to_uppercase().as_str() {
            "FROM" => config.base_image = parts[1].to_string(),
            "RUN" => config.commands.push(DockerCommand::Run {
                command: parts[1].to_string(),
            }),
            "COPY" => {
                let copy_parts: Vec<&str> = parts[1].split(' ').collect();
                if copy_parts.len() == 2 {
                    config.commands.push(DockerCommand::Copy {
                        source: copy_parts[0].to_string(),
                        dest: copy_parts[1].to_string(),
                    });
                }
            }
            "USER" => config.commands.push(DockerCommand::Run {
                command: parts[1].to_string(),
            }),
            "EXPOSE" => {
                if let Ok(port) = parts[1].parse() {
                    config.commands.push(DockerCommand::Expose { port });
                }
            }
            _ => {}
        }

        Ok(())
    }
}
