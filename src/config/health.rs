use crate::error::DockerError;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[cfg(feature = "docker")]
use reqwest::Client;

#[cfg(feature = "docker")]
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub endpoint: String,
    pub method: String,
    pub expected_status: u16,
    pub body: Option<String>,
    #[serde(with = "duration_serde")]
    pub interval: Duration,
    #[serde(with = "duration_serde")]
    pub timeout: Duration,
    pub retries: u32,
}

// Custom serialization for Duration
pub(crate) mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_nanos().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let nanos = u128::deserialize(deserializer)?;
        Ok(Duration::from_nanos(nanos as u64))
    }
}

#[cfg(feature = "docker")]
impl HealthCheck {
    pub async fn check(&self) -> Result<(), DockerError> {
        let client = Client::new();
        let mut attempts = 0;

        while attempts < self.retries {
            match self.perform_check(&client).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    attempts += 1;
                    if attempts == self.retries {
                        return Err(e);
                    }
                    sleep(self.interval).await;
                }
            }
        }

        Ok(())
    }

    async fn perform_check(&self, client: &Client) -> Result<(), DockerError> {
        let mut request = match self.method.to_uppercase().as_str() {
            "GET" => client.get(&self.endpoint),
            "POST" => client.post(&self.endpoint),
            _ => {
                return Err(DockerError::ValidationError(
                    "Unsupported HTTP method".into(),
                ));
            }
        };

        if let Some(body) = &self.body {
            request = request.body(body.clone());
        }

        let response = request
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| DockerError::ValidationError(format!("Health check failed: {}", e)))?;

        if response.status().as_u16() != self.expected_status {
            return Err(DockerError::ValidationError(format!(
                "Health check failed: expected status {}, got {}",
                self.expected_status,
                response.status()
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_health_check_success() {
        let health_check = HealthCheck {
            endpoint: "https://httpbin.org/status/200".to_string(),
            method: "GET".to_string(),
            expected_status: 200,
            body: None,
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(5),
            retries: 3,
        };

        assert!(health_check.check().await.is_ok());
    }

    #[tokio::test]
    async fn test_health_check_failure() {
        let health_check = HealthCheck {
            endpoint: "https://httpbin.org/status/500".to_string(),
            method: "GET".to_string(),
            expected_status: 200,
            body: None,
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(5),
            retries: 2,
        };

        assert!(health_check.check().await.is_err());
    }
}
