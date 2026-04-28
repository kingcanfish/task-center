use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelSelector {
    required: BTreeMap<String, String>,
}

impl LabelSelector {
    pub fn parse(input: &str) -> Result<Self> {
        let mut required = BTreeMap::new();
        for part in input
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            let (key, value) = part
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid label selector `{part}`, expected key=value"))?;
            let key = key.trim();
            let value = value.trim();
            if key.is_empty() || value.is_empty() {
                return Err(anyhow!(
                    "invalid label selector `{part}`, key and value must be non-empty"
                ));
            }
            required.insert(key.to_string(), value.to_string());
        }
        Ok(Self { required })
    }

    pub fn matches(&self, labels: &BTreeMap<String, String>) -> bool {
        self.required
            .iter()
            .all(|(key, value)| labels.get(key) == Some(value))
    }

    pub fn required(&self) -> &BTreeMap<String, String> {
        &self.required
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn selector_matches_all_required_labels() {
        let selector = LabelSelector::parse("executor=shell,trusted=true").unwrap();
        let labels = BTreeMap::from([
            ("executor".to_string(), "shell".to_string()),
            ("trusted".to_string(), "true".to_string()),
            ("region".to_string(), "cn".to_string()),
        ]);
        assert!(selector.matches(&labels));
    }

    #[test]
    fn selector_rejects_missing_label() {
        let selector = LabelSelector::parse("executor=shell,trusted=true").unwrap();
        let labels = BTreeMap::from([("executor".to_string(), "shell".to_string())]);
        assert!(!selector.matches(&labels));
    }
}
