use crate::{config::compose::ComposeConfig, error::DockerError};
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;

pub struct ComposeParser;

impl ComposeParser {
    pub fn parse(content: &str) -> Result<ComposeConfig, DockerError> {
        let mut config: ComposeConfig =
            serde_yaml::from_str(content).map_err(DockerError::YamlError)?;
        config.normalize();
        Ok(config)
    }

    pub fn parse_with_env(content: &str, env_file: &Path) -> Result<ComposeConfig, DockerError> {
        // First read and parse the env file
        let env_content =
            std::fs::read_to_string(env_file).map_err(|e| DockerError::FileError(e))?;

        let env_vars = Self::parse_env_file(&env_content)?;

        // Substitute environment variables in the content
        let processed_content = Self::substitute_env_vars(content, &env_vars)?;

        // Parse the processed content
        let mut config = Self::parse(&processed_content)?;
        config.normalize();
        Ok(config)
    }

    fn parse_env_file(content: &str) -> Result<HashMap<String, String>, DockerError> {
        let mut vars = HashMap::new();
        // Add regex for valid environment variable names
        let valid_key = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                // Only add if key is valid
                if valid_key.is_match(key) {
                    vars.insert(key.to_string(), value.trim().trim_matches('"').to_string());
                }
            }
        }

        Ok(vars)
    }

    fn substitute_env_vars(
        content: &str,
        env_vars: &HashMap<String, String>,
    ) -> Result<String, DockerError> {
        let mut result = content.to_string();

        // Handle ${VAR:-default} syntax
        let re_with_default = Regex::new(r"\$\{([^{}:]+):-([^{}]*)\}").unwrap(); // Note the * instead of + for empty defaults
        result = re_with_default
            .replace_all(&result, |caps: &regex::Captures| {
                let var_name = caps.get(1).unwrap().as_str();
                let default_value = caps.get(2).unwrap().as_str();
                match env_vars.get(var_name) {
                    Some(value) if value.is_empty() => default_value.to_string(),
                    Some(value) => value.to_string(),
                    None => default_value.to_string(),
                }
            })
            .to_string();

        // Handle ${VAR} syntax
        let re_simple = Regex::new(r"\$\{([^{}]+)\}").unwrap();
        result = re_simple
            .replace_all(&result, |caps: &regex::Captures| {
                let var_name = caps.get(1).unwrap().as_str();
                env_vars
                    .get(var_name)
                    .map(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .to_string();

        // Handle $VAR syntax
        let re_basic = Regex::new(r"\$([a-zA-Z_][a-zA-Z0-9_]*)").unwrap();
        result = re_basic
            .replace_all(&result, |caps: &regex::Captures| {
                let var_name = caps.get(1).unwrap().as_str();
                env_vars
                    .get(var_name)
                    .map(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .to_string();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_basic_env_substitution() {
        let mut env_vars = HashMap::new();
        env_vars.insert("IMAGE_TAG__L2GETH".to_string(), "v1.0.0".to_string());
        env_vars.insert("SIMPLE_VAR".to_string(), "value".to_string());

        let content = r#"
        services:
          l2geth:
            image: ethereumoptimism/l2geth:${IMAGE_TAG__L2GETH:-latest}
          other:
            image: something:${UNDEFINED_VAR:-default}
          simple:
            value: $SIMPLE_VAR
        "#;

        let result = ComposeParser::substitute_env_vars(content, &env_vars).unwrap();

        assert!(result.contains("ethereumoptimism/l2geth:v1.0.0"));
        assert!(result.contains("something:default"));
        assert!(result.contains("value: value"));
    }

    #[test]
    fn test_nested_env_substitution() {
        let mut env_vars = HashMap::new();
        env_vars.insert("PORT".to_string(), "8545".to_string());
        env_vars.insert("HOST".to_string(), "localhost".to_string());

        let content = r#"
        services:
          geth:
            ports:
              - "${PORT:-3000}:${PORT:-3000}"
            environment:
              - URL=http://${HOST:-127.0.0.1}:${PORT:-3000}
              - SIMPLE=$HOST:$PORT
        "#;

        let result = ComposeParser::substitute_env_vars(content, &env_vars).unwrap();

        assert!(result.contains("8545:8545"));
        assert!(result.contains("http://localhost:8545"));
        assert!(result.contains("SIMPLE=localhost:8545"));
    }

    #[test]
    fn test_env_file_parsing() {
        let env_content = r#"
        # Comment line
        EMPTY=
        QUOTED="quoted value"
        UNQUOTED=unquoted value
        WITH_SPACES=  spaced value  
        "#;

        let temp_file = NamedTempFile::new().unwrap();
        fs::write(&temp_file, env_content).unwrap();

        let vars = ComposeParser::parse_env_file(env_content).unwrap();

        assert_eq!(vars.get("EMPTY").unwrap(), "");
        assert_eq!(vars.get("QUOTED").unwrap(), "quoted value");
        assert_eq!(vars.get("UNQUOTED").unwrap(), "unquoted value");
        assert_eq!(vars.get("WITH_SPACES").unwrap(), "spaced value");
    }

    #[test]
    fn test_complex_substitutions() {
        let mut env_vars = HashMap::new();
        env_vars.insert("VERSION".to_string(), "1.0".to_string());
        env_vars.insert("MEMORY".to_string(), "1G".to_string());

        let content = r#"
        services:
          app:
            image: myapp:${VERSION:-latest}
            deploy:
              resources:
                limits:
                  memory: ${MEMORY:-512M}
                  cpus: ${CPUS:-1.0}
            environment:
              - CONFIG=${CONFIG_PATH:-/etc/config}
              - COMBINED=${VERSION:-0.0.1}-${MEMORY:-256M}
        "#;

        let result = ComposeParser::substitute_env_vars(content, &env_vars).unwrap();

        assert!(result.contains("myapp:1.0"));
        assert!(result.contains("memory: 1G"));
        assert!(result.contains("cpus: 1.0")); // Uses default
        assert!(result.contains("CONFIG=/etc/config")); // Uses default
        assert!(result.contains("COMBINED=1.0-1G"));
    }

    #[test]
    fn test_invalid_env_file() {
        let env_content = r#"
        VALID_KEY=value
        INVALID+KEY=value
        123INVALID=value
        _VALID=value
        ALSO-INVALID=value
        "#;
        let vars = ComposeParser::parse_env_file(env_content).unwrap();

        assert!(vars.contains_key("VALID_KEY"));
        assert!(vars.contains_key("_VALID"));
        assert!(!vars.contains_key("INVALID+KEY"));
        assert!(!vars.contains_key("123INVALID"));
        assert!(!vars.contains_key("ALSO-INVALID"));
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_empty_and_missing_variables() {
        let mut env_vars = HashMap::new();
        env_vars.insert("EMPTY".to_string(), "".to_string());

        let content = r#"
        services:
          app:
            image: test:${EMPTY:-default}
            command: ${MISSING}
            environment:
              - UNSET=${UNDEFINED:-}
              - WITH_DEFAULT=${UNDEFINED:-default_value}
        "#;

        let result = ComposeParser::substitute_env_vars(content, &env_vars).unwrap();

        assert!(
            result.contains("test:default"),
            "Empty var should use default"
        );
        assert!(result.contains("command: "), "Missing var should be empty");
        assert!(
            result.contains("UNSET="),
            "Undefined with empty default should be empty"
        );
        assert!(
            result.contains("WITH_DEFAULT=default_value"),
            "Undefined should use default value"
        );
    }

    #[test]
    fn test_empty_default_values() {
        let env_vars = HashMap::new();
        let content = r#"
        TEST1=${VAR:-}
        TEST2=${OTHER_VAR:-default}
        "#;

        let result = ComposeParser::substitute_env_vars(content, &env_vars).unwrap();
        assert!(result.contains("TEST1="));
        assert!(result.contains("TEST2=default"));
    }

    #[test]
    fn test_full_compose_parsing() {
        let compose_content = r#"
        version: "3.8"
        services:
          web:
            image: nginx:${NGINX_VERSION:-latest}
            ports:
              - "${PORT:-80}:80"
        "#;

        let env_content = "NGINX_VERSION=1.21\nPORT=8080";
        let temp_file = NamedTempFile::new().unwrap();
        fs::write(&temp_file, env_content).unwrap();

        let config = ComposeParser::parse_with_env(compose_content, temp_file.path()).unwrap();

        if let Some(web_service) = config.services.get("web") {
            assert_eq!(web_service.image.as_deref().unwrap(), "nginx:1.21");
            assert_eq!(
                web_service.ports.as_ref().unwrap().first().unwrap(),
                "8080:80"
            );
        } else {
            panic!("Web service not found in parsed config");
        }
    }
}
