use crate::{config::compose::ComposeConfig, error::DockerError};
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Parser for Docker Compose configuration files with environment variable support
///
/// This parser handles:
/// - YAML parsing of Docker Compose files
/// - Environment variable substitution with default values
/// - Loading and parsing of .env files
/// - Path normalization
///
/// # Examples
///
/// ```rust,no_run
/// use std::path::Path;
/// use dockworker::parser::ComposeParser;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Parse a compose file with environment variables from a .env file
/// let config = ComposeParser::from_file_with_env(
///     Path::new("docker-compose.yml"),
///     Path::new(".env")
/// ).await?;
///
/// // Parse a compose file with explicit environment variables
/// let mut env_vars = std::collections::HashMap::new();
/// env_vars.insert("VERSION".to_string(), "1.0".to_string());
/// let config = ComposeParser::from_file_with_env_map(
///     Path::new("docker-compose.yml"),
///     &env_vars
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub struct ComposeParser;

impl ComposeParser {
    /// Parses a Docker Compose file from the given path
    ///
    /// This is the simplest way to parse a compose file when no environment
    /// variable substitution is needed.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the Docker Compose file
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the parsed `ComposeConfig` or a `DockerError`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use std::path::Path;
    /// # use dockworker::parser::ComposeParser;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = ComposeParser::from_file(Path::new("docker-compose.yml")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<ComposeConfig, DockerError> {
        let content = fs::read_to_string(path.as_ref())
            .await
            .map_err(DockerError::FileError)?;
        Self::parse(&content)
    }

    /// Parses a Docker Compose file with environment variables from an env file
    ///
    /// This method reads both the compose file and an environment file, then
    /// performs variable substitution before parsing.
    ///
    /// # Arguments
    ///
    /// * `compose_path` - Path to the Docker Compose file
    /// * `env_path` - Path to the environment file
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the parsed `ComposeConfig` or a `DockerError`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use std::path::Path;
    /// # use dockworker::parser::ComposeParser;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = ComposeParser::from_file_with_env(
    ///     Path::new("docker-compose.yml"),
    ///     Path::new(".env")
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_file_with_env<P: AsRef<Path>>(
        compose_path: P,
        env_path: P,
    ) -> Result<ComposeConfig, DockerError> {
        let content = fs::read_to_string(compose_path.as_ref())
            .await
            .map_err(DockerError::FileError)?;
        let env_content = fs::read_to_string(env_path.as_ref())
            .await
            .map_err(DockerError::FileError)?;

        let env_vars = Self::parse_env_file(&env_content)?;
        let processed_content = Self::substitute_env_vars(&content, &env_vars)?;

        let config = Self::parse(&processed_content)?;
        Ok(config)
    }

    /// Parses a Docker Compose file with environment variables from a HashMap
    ///
    /// This method is useful when you want to provide environment variables
    /// programmatically rather than from a file.
    ///
    /// # Arguments
    ///
    /// * `compose_path` - Path to the Docker Compose file
    /// * `env_vars` - HashMap containing environment variable key-value pairs
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the parsed `ComposeConfig` or a `DockerError`
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use std::path::Path;
    /// # use std::collections::HashMap;
    /// # use dockworker::parser::ComposeParser;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut env_vars = HashMap::new();
    /// env_vars.insert("VERSION".to_string(), "1.0".to_string());
    /// let config = ComposeParser::from_file_with_env_map(
    ///     Path::new("docker-compose.yml"),
    ///     &env_vars
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_file_with_env_map<P: AsRef<Path>>(
        compose_path: P,
        env_vars: &HashMap<String, String>,
    ) -> Result<ComposeConfig, DockerError> {
        let content = fs::read_to_string(compose_path.as_ref())
            .await
            .map_err(DockerError::FileError)?;
        let processed_content = Self::substitute_env_vars(&content, env_vars)?;

        let config = Self::parse(&processed_content)?;
        Ok(config)
    }

    /// Parses a Docker Compose configuration from a string
    ///
    /// This is the core parsing method that other methods build upon.
    /// It handles the basic YAML parsing and normalization.
    pub fn parse(content: &str) -> Result<ComposeConfig, DockerError> {
        let config: ComposeConfig =
            serde_yaml::from_str(content).map_err(DockerError::YamlError)?;
        Ok(config)
    }

    /// Parses an environment file into a HashMap of key-value pairs
    ///
    /// This method handles:
    /// - Comments (lines starting with #)
    /// - Empty lines
    /// - KEY=value format
    /// - Quoted values
    /// - Validation of environment variable names
    fn parse_env_file(content: &str) -> Result<HashMap<String, String>, DockerError> {
        let mut vars = HashMap::new();
        let valid_key = Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                if valid_key.is_match(key) {
                    vars.insert(key.to_string(), value.trim().trim_matches('"').to_string());
                }
            }
        }

        Ok(vars)
    }

    /// Substitutes environment variables in a string
    ///
    /// Supports the following formats:
    /// - ${VAR}
    /// - ${VAR:-default}
    /// - $VAR
    ///
    /// # Arguments
    ///
    /// * `content` - The string containing environment variable references
    /// * `env_vars` - HashMap of environment variables to use for substitution
    fn substitute_env_vars(
        content: &str,
        env_vars: &HashMap<String, String>,
    ) -> Result<String, DockerError> {
        let mut result = content.to_string();

        // Handle ${VAR:-default} syntax
        let re_with_default = Regex::new(r"\$\{([^{}:]+):-([^{}]*)\}").unwrap();
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

    pub fn parse_with_env(content: &str, env_path: &Path) -> Result<ComposeConfig, DockerError> {
        // Read environment variables from file
        let env_content = std::fs::read_to_string(env_path)
            .map_err(|e| DockerError::ValidationError(format!("Failed to read env file: {}", e)))?;

        // Parse environment variables using existing function
        let env_vars = Self::parse_env_file(&env_content)?;

        // Substitute environment variables in the content
        let content = Self::substitute_env_vars(content, &env_vars)?;

        // Parse the content with substituted environment variables
        Self::parse(&content)
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
              requirements:
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
