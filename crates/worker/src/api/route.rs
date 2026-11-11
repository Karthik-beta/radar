use domain::api::ItemId;
use worker::Method;

/// The inbound `/api/` endpoints. Dismiss and snooze are POSTs from Radar's
/// own pages that write `local_state` in D1; nothing here sends a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    Health,
    Protected(Protected),
    WrongMethod,
    NotFound,
}

/// The endpoints that need a signed-in session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protected {
    Dashboard,
    Feed,
    Repos,
    Freshness,
    Dismiss(ItemId),
    Snooze(ItemId),
}

impl Endpoint {
    /// `None` means the path belongs to the static assets.
    pub fn parse(method: &Method, path: &str) -> Option<Self> {
        if !path.starts_with("/api/") && !path.starts_with("/__") {
            return None;
        }
        let (allowed, endpoint) = match path {
            "/api/health" => (Method::Get, Self::Health),
            "/api/dashboard" => (Method::Get, Self::Protected(Protected::Dashboard)),
            "/api/feed" => (Method::Get, Self::Protected(Protected::Feed)),
            "/api/freshness" => (Method::Get, Self::Protected(Protected::Freshness)),
            "/api/repos" => (Method::Get, Self::Protected(Protected::Repos)),
            _ => match item_action(path) {
                Some(action) => (Method::Post, Self::Protected(action)),
                None => return Some(Self::NotFound),
            },
        };
        Some(if *method == allowed {
            endpoint
        } else {
            Self::WrongMethod
        })
    }
}

impl Protected {
    pub fn is_post(&self) -> bool {
        matches!(self, Self::Dismiss(_) | Self::Snooze(_))
    }
}

/// `/api/items/<id>/dismiss` or `/api/items/<id>/snooze`.
fn item_action(path: &str) -> Option<Protected> {
    let rest = path.strip_prefix("/api/items/")?;
    let (raw, action) = rest.split_once('/')?;
    let id = ItemId::parse(raw).ok()?;
    match action {
        "dismiss" => Some(Protected::Dismiss(id)),
        "snooze" => Some(Protected::Snooze(id)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> ItemId {
        ItemId::parse(s).expect("id")
    }

    #[test]
    fn each_endpoint_answers_one_method() {
        let get = Method::Get;
        let post = Method::Post;
        assert_eq!(Endpoint::parse(&get, "/api/health"), Some(Endpoint::Health));
        let protected = |p| Some(Endpoint::Protected(p));
        assert_eq!(
            Endpoint::parse(&get, "/api/dashboard"),
            protected(Protected::Dashboard)
        );
        assert_eq!(
            Endpoint::parse(&get, "/api/feed"),
            protected(Protected::Feed)
        );
        assert_eq!(
            Endpoint::parse(&post, "/api/feed"),
            Some(Endpoint::WrongMethod)
        );
        assert_eq!(
            Endpoint::parse(&get, "/api/freshness"),
            protected(Protected::Freshness)
        );
        assert_eq!(
            Endpoint::parse(&get, "/api/repos"),
            protected(Protected::Repos)
        );
        assert_eq!(
            Endpoint::parse(&post, "/api/items/PR_1/dismiss"),
            protected(Protected::Dismiss(id("PR_1")))
        );
        assert_eq!(
            Endpoint::parse(&post, "/api/items/PR_1/snooze"),
            protected(Protected::Snooze(id("PR_1")))
        );
        assert_eq!(
            Endpoint::parse(&post, "/api/dashboard"),
            Some(Endpoint::WrongMethod)
        );
        assert_eq!(
            Endpoint::parse(&get, "/api/items/PR_1/dismiss"),
            Some(Endpoint::WrongMethod)
        );
        assert!(Protected::Snooze(id("PR_1")).is_post());
        assert!(!Protected::Dashboard.is_post());
    }

    #[test]
    fn unknown_api_paths_are_not_found() {
        for path in [
            "/api/nope",
            "/api/items/PR_1",
            "/api/items/PR_1/archive",
            "/api/items//dismiss",
            "/api/items/PR 1/dismiss",
            "/api/items/a/b/dismiss",
            "/api/dashboard/",
            "/__nope",
        ] {
            assert_eq!(
                Endpoint::parse(&Method::Post, path),
                Some(Endpoint::NotFound),
                "{path}"
            );
        }
    }

    #[test]
    fn other_paths_go_to_assets() {
        assert_eq!(Endpoint::parse(&Method::Get, "/"), None);
        assert_eq!(Endpoint::parse(&Method::Get, "/some/deep/link"), None);
        assert_eq!(Endpoint::parse(&Method::Get, "/api"), None);
    }
}
