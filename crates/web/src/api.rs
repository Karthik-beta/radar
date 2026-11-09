use domain::api::{Dashboard, Freshness, ItemId, SnoozeFor, SnoozeRequest};
use gloo_net::http::{Request, Response};
use serde::de::DeserializeOwned;

/// Why a call to the Worker's `/api/` failed. `Unauthorized` means the
/// session is gone, and the app shows the sign-in view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiError {
    Unauthorized,
    Status(u16),
    Network,
    Decode,
}

pub async fn dashboard() -> Result<Dashboard, ApiError> {
    get("/api/dashboard").await
}

pub async fn freshness() -> Result<Freshness, ApiError> {
    get("/api/freshness").await
}

/// The browser adds the same-origin `Origin` header the Worker checks.
pub async fn dismiss(id: &ItemId) -> Result<(), ApiError> {
    let request = Request::post(&format!("/api/items/{id}/dismiss"));
    accept(request.send().await.map_err(|_| ApiError::Network)?)?;
    Ok(())
}

pub async fn snooze(id: &ItemId, snooze_for: SnoozeFor) -> Result<(), ApiError> {
    let request = Request::post(&format!("/api/items/{id}/snooze"))
        .json(&SnoozeRequest { snooze_for })
        .map_err(|_| ApiError::Decode)?;
    accept(request.send().await.map_err(|_| ApiError::Network)?)?;
    Ok(())
}

async fn get<T: DeserializeOwned>(path: &str) -> Result<T, ApiError> {
    let response = Request::get(path)
        .send()
        .await
        .map_err(|_| ApiError::Network)?;
    accept(response)?
        .json::<T>()
        .await
        .map_err(|_| ApiError::Decode)
}

fn accept(response: Response) -> Result<Response, ApiError> {
    match response.status() {
        200..=299 => Ok(response),
        401 => Err(ApiError::Unauthorized),
        status => Err(ApiError::Status(status)),
    }
}
