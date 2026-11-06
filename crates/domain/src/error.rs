use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ErrorBody {
    pub error: ErrorCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_uses_snake_case_codes() {
        let not_found = ErrorBody {
            error: ErrorCode::NotFound,
        };
        assert_eq!(
            serde_json::to_string(&not_found).unwrap(),
            r#"{"error":"not_found"}"#
        );
        let internal = ErrorBody {
            error: ErrorCode::Internal,
        };
        assert_eq!(
            serde_json::to_string(&internal).unwrap(),
            r#"{"error":"internal"}"#
        );
    }
}
