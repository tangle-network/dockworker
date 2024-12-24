use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentVars(HashMap<String, String>);

impl<'de> Deserialize<'de> for EnvironmentVars {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let value = serde_yaml::Value::deserialize(deserializer)?;

        let map = match value {
            // If it's a mapping, convert directly to HashMap
            serde_yaml::Value::Mapping(map) => {
                let mut env_map = HashMap::new();
                for (key, value) in map {
                    let key = key
                        .as_str()
                        .ok_or_else(|| Error::custom("Environment key must be a string"))?
                        .to_string();
                    let value = value
                        .as_str()
                        .ok_or_else(|| Error::custom("Environment value must be a string"))?
                        .trim_matches('"')
                        .to_string();
                    env_map.insert(key, value);
                }
                env_map
            }
            // If it's a sequence, parse each item as KEY=VALUE
            serde_yaml::Value::Sequence(seq) => {
                let mut env_map = HashMap::new();
                for item in seq {
                    let item_str = item
                        .as_str()
                        .ok_or_else(|| Error::custom("Environment list item must be a string"))?;
                    if let Some((key, value)) = item_str.split_once('=') {
                        env_map.insert(
                            key.trim().to_string(),
                            value.trim().trim_matches('"').to_string(),
                        );
                    }
                }
                env_map
            }
            _ => return Err(Error::custom("Environment must be a mapping or sequence")),
        };

        Ok(EnvironmentVars(map))
    }
}

impl Default for EnvironmentVars {
    fn default() -> Self {
        EnvironmentVars(HashMap::new())
    }
}

impl EnvironmentVars {
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.0.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut String> {
        self.0.values_mut()
    }
}

impl From<EnvironmentVars> for HashMap<String, String> {
    fn from(env: EnvironmentVars) -> Self {
        env.0
    }
}

impl From<HashMap<String, String>> for EnvironmentVars {
    fn from(map: HashMap<String, String>) -> Self {
        EnvironmentVars(map)
    }
}

impl IntoIterator for EnvironmentVars {
    type Item = (String, String);
    type IntoIter = std::collections::hash_map::IntoIter<String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a EnvironmentVars {
    type Item = (&'a String, &'a String);
    type IntoIter = std::collections::hash_map::Iter<'a, String, String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
