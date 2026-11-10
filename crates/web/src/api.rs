use domain::agents::AgentsPage;
use domain::api::{Dashboard, Freshness, ItemId, SnoozeFor, SnoozeRequest};
use domain::feed::{FeedCursor, FeedPage};
use gloo_net::http::{Request, Response};
use serde::de::DeserializeOwned;

use crate::route::{encode, FeedFilter};

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

pub async fn agents() -> Result<AgentsPage, ApiError> {
    get("/api/agents").await
}

pub async fn feed(filter: &FeedFilter, before: Option<&FeedCursor>) -> Result<FeedPage, ApiError> {
    get(&feed_path(filter, before)).await
}

fn feed_path(filter: &FeedFilter, before: Option<&FeedCursor>) -> String {
    let mut query = filter.query();
    if let Some(cursor) = before {
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str(&format!("before={}", encode(&cursor.to_string())));
    }
    if query.is_empty() {
        "/api/feed".to_owned()
    } else {
        format!("/api/feed?{query}")
    }
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

#[cfg(test)]
mod tests {
    use domain::ids::Timestamp;

    use super::*;

    #[test]
    fn the_feed_path_carries_the_filter_and_the_cursor() {
        assert_eq!(feed_path(&FeedFilter::default(), None), "/api/feed");
        let filter = FeedFilter::from_params(Some("o/r"), Some("merged"));
        let cursor = FeedCursor {
            at: Timestamp(1_793_000_000),
            id: "ci:PR_1:abc:failed".into(),
        };
        assert_eq!(
            feed_path(&filter, Some(&cursor)),
            "/api/feed?repo=o/r&kind=merged&before=1793000000_ci%3APR_1%3Aabc%3Afailed"
        );
        assert_eq!(
            feed_path(&FeedFilter::default(), Some(&cursor)),
            "/api/feed?before=1793000000_ci%3APR_1%3Aabc%3Afailed"
        );
    }
}
