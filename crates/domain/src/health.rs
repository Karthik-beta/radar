use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Health {
    pub status: HealthStatus,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Ok,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_serializes_status_and_version() {
        let health = Health {
            status: HealthStatus::Ok,
            version: "0.0.0".into(),
        };
        let json = serde_json::to_string(&health).unwrap();
        assert_eq!(json, r#"{"status":"ok","version":"0.0.0"}"#);
        assert_eq!(serde_json::from_str::<Health>(&json).unwrap(), health);
    }

    #[test]
    fn unknown_status_is_rejected() {
        let parsed = serde_json::from_str::<Health>(r#"{"status":"degraded","version":"0.0.0"}"#);
        assert!(parsed.is_err());
    }
}
