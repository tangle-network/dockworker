use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum VolumeType {
    Named(String),
    Bind {
        source: PathBuf,
        target: String,
        read_only: bool,
    },
}

impl fmt::Display for VolumeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VolumeType::Named(name) => write!(f, "{}", name),
            VolumeType::Bind {
                source,
                target,
                read_only,
            } => {
                if *read_only {
                    write!(f, "{}:{}:ro", source.display(), target)
                } else {
                    write!(f, "{}:{}", source.display(), target)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VolumeSpec {
    Short(String),
    Long {
        #[serde(rename = "type")]
        volume_type: String,
        source: String,
        target: String,
        read_only: Option<bool>,
    },
}

impl From<VolumeSpec> for VolumeType {
    fn from(spec: VolumeSpec) -> Self {
        match spec {
            VolumeSpec::Short(s) => {
                let parts: Vec<&str> = s.split(':').collect();
                match parts.len() {
                    2 => {
                        if parts[0].starts_with('/') || parts[0].starts_with('.') {
                            VolumeType::Bind {
                                source: PathBuf::from(parts[0]),
                                target: parts[1].to_string(),
                                read_only: false,
                            }
                        } else {
                            VolumeType::Named(s)
                        }
                    }
                    3 if parts[2] == "ro" => VolumeType::Bind {
                        source: PathBuf::from(parts[0]),
                        target: parts[1].to_string(),
                        read_only: true,
                    },
                    _ => VolumeType::Named(s),
                }
            }
            VolumeSpec::Long {
                volume_type,
                source,
                target,
                read_only,
            } => match volume_type.as_str() {
                "bind" => VolumeType::Bind {
                    source: PathBuf::from(source),
                    target,
                    read_only: read_only.unwrap_or(false),
                },
                _ => VolumeType::Named(format!("{}:{}", source, target)),
            },
        }
    }
}

impl<'de> Deserialize<'de> for VolumeType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let spec = VolumeSpec::deserialize(deserializer)?;
        Ok(VolumeType::from(spec))
    }
}

impl Serialize for VolumeType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_volume_format() {
        let yaml = r#"
            - named_vol:/data
            - ./local:/container
            - /abs/path:/container/config:ro
        "#;

        let volumes: Vec<VolumeType> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(volumes.len(), 3);

        assert!(matches!(&volumes[0], VolumeType::Named(name) if name == "named_vol:/data"));

        match &volumes[1] {
            VolumeType::Bind {
                source,
                target,
                read_only,
            } => {
                assert_eq!(source, &PathBuf::from("./local"));
                assert_eq!(target, "/container");
                assert!(!read_only);
            }
            _ => panic!("Expected bind mount"),
        }

        match &volumes[2] {
            VolumeType::Bind {
                source,
                target,
                read_only,
            } => {
                assert_eq!(source, &PathBuf::from("/abs/path"));
                assert_eq!(target, "/container/config");
                assert!(*read_only);
            }
            _ => panic!("Expected bind mount"),
        }
    }

    #[test]
    fn test_long_volume_format() {
        let yaml = r#"
            - type: volume
              source: named_vol
              target: /data
            - type: bind
              source: ./local
              target: /container
            - type: bind
              source: /abs/path
              target: /container/config
              read_only: true
        "#;

        let volumes: Vec<VolumeType> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(volumes.len(), 3);

        assert!(matches!(&volumes[0], VolumeType::Named(name) if name == "named_vol:/data"));

        match &volumes[1] {
            VolumeType::Bind {
                source,
                target,
                read_only,
            } => {
                assert_eq!(source, &PathBuf::from("./local"));
                assert_eq!(target, "/container");
                assert!(!read_only);
            }
            _ => panic!("Expected bind mount"),
        }

        match &volumes[2] {
            VolumeType::Bind {
                source,
                target,
                read_only,
            } => {
                assert_eq!(source, &PathBuf::from("/abs/path"));
                assert_eq!(target, "/container/config");
                assert!(*read_only);
            }
            _ => panic!("Expected bind mount"),
        }
    }

    #[test]
    fn test_roundtrip() {
        let yaml = r#"
            - named_vol:/data
            - /abs/path:/container
            - /data:/container:ro
        "#;

        let volumes: Vec<VolumeType> = serde_yaml::from_str(yaml).unwrap();
        let serialized = serde_yaml::to_string(&volumes).unwrap();
        let deserialized: Vec<VolumeType> = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(volumes, deserialized);
    }

    #[test]
    fn test_display() {
        let named = VolumeType::Named("myvolume:/data".to_string());
        assert_eq!(named.to_string(), "myvolume:/data");

        let bind = VolumeType::Bind {
            source: PathBuf::from("/host/path"),
            target: "/container/path".to_string(),
            read_only: false,
        };
        assert_eq!(bind.to_string(), "/host/path:/container/path");

        let bind_ro = VolumeType::Bind {
            source: PathBuf::from("/host/path"),
            target: "/container/path".to_string(),
            read_only: true,
        };
        assert_eq!(bind_ro.to_string(), "/host/path:/container/path:ro");
    }
}
