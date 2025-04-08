use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::time::Duration;

#[cfg(feature = "deploy")]
use reqwest::Client;

#[cfg(feature = "deploy")]
use tokio::time::sleep;

#[cfg(feature = "deploy")]
#[derive(Debug, thiserror::Error)]
pub enum HealthCheckError {
    #[error("Health check failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Health check failed: expected status {expected}, got {actual}")]
    UnexpectedStatus { expected: u16, actual: u16 },
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum Method {
    Get,
    Post,
}

impl Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Get => write!(f, "GET"),
            Method::Post => write!(f, "POST"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub endpoint: String,
    pub method: Method,
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

#[cfg(feature = "deploy")]
impl HealthCheck {
    pub async fn check(&self) -> Result<(), HealthCheckError> {
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

    async fn perform_check(&self, client: &Client) -> Result<(), HealthCheckError> {
        let mut request = match self.method {
            Method::Get => client.get(&self.endpoint),
            Method::Post => client.post(&self.endpoint),
        };

        if let Some(body) = &self.body {
            request = request.body(body.clone());
        }

        let response = request
            .timeout(self.timeout)
            .send()
            .await
            .map_err(HealthCheckError::Request)?;

        let status = response.status().as_u16();
        if status != self.expected_status {
            return Err(HealthCheckError::UnexpectedStatus {
                expected: self.expected_status,
                actual: status,
            });
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
            method: Method::Get,
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
            method: Method::Get,
            expected_status: 200,
            body: None,
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(5),
            retries: 2,
        };

        assert!(health_check.check().await.is_err());
    }
}
