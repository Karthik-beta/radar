use domain::error::{ErrorBody, ErrorCode};
use domain::health::{Health, HealthStatus};

#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    Json { status: u16, body: String },
    Assets,
}

pub fn route(method: &str, path: &str) -> Route {
    match (method, path) {
        ("GET", "/api/health") => json(
            200,
            &Health {
                status: HealthStatus::Ok,
                version: env!("CARGO_PKG_VERSION").into(),
            },
        ),
        _ if path.starts_with("/api/") || path.starts_with("/__") => {
            error(404, ErrorCode::NotFound)
        }
        _ => Route::Assets,
    }
}

fn error(status: u16, code: ErrorCode) -> Route {
    json(status, &ErrorBody { error: code })
}

fn json<T: serde::Serialize>(status: u16, value: &T) -> Route {
    match serde_json::to_string(value) {
        Ok(body) => Route::Json { status, body },
        Err(_) => Route::Json {
            status: 500,
            body: r#"{"error":"internal"}"#.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn not_found() -> Route {
        Route::Json {
            status: 404,
            body: r#"{"error":"not_found"}"#.into(),
        }
    }

    #[test]
    fn unknown_api_path_maps_to_not_found() {
        assert_eq!(route("GET", "/api/nope"), not_found());
        assert_eq!(route("POST", "/api/health"), not_found());
    }

    #[test]
    fn unknown_double_underscore_path_maps_to_not_found() {
        assert_eq!(route("GET", "/__nope"), not_found());
    }

    #[test]
    fn health_returns_status_and_version() {
        assert_eq!(
            route("GET", "/api/health"),
            Route::Json {
                status: 200,
                body: r#"{"status":"ok","version":"0.0.0"}"#.into(),
            }
        );
    }

    #[test]
    fn other_paths_go_to_assets() {
        assert_eq!(route("GET", "/"), Route::Assets);
        assert_eq!(route("GET", "/some/deep/link"), Route::Assets);
    }
}
