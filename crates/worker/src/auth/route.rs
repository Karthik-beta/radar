use worker::Method;

/// The inbound auth endpoints. Logout and disconnect are POSTs from Radar's
/// own pages; nothing here sends a request anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Login,
    Callback,
    Logout,
    Disconnect,
    Me,
    WrongMethod,
}

impl Route {
    pub fn parse(method: &Method, path: &str) -> Option<Self> {
        let (allowed, route) = match path {
            "/auth/login" => (Method::Get, Self::Login),
            "/auth/callback" => (Method::Get, Self::Callback),
            "/auth/logout" => (Method::Post, Self::Logout),
            "/auth/disconnect" => (Method::Post, Self::Disconnect),
            "/api/me" => (Method::Get, Self::Me),
            _ => return None,
        };
        Some(if *method == allowed {
            route
        } else {
            Self::WrongMethod
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_endpoint_answers_one_method() {
        assert_eq!(
            Route::parse(&Method::Get, "/auth/login"),
            Some(Route::Login)
        );
        assert_eq!(
            Route::parse(&Method::Get, "/auth/callback"),
            Some(Route::Callback)
        );
        assert_eq!(
            Route::parse(&Method::Post, "/auth/logout"),
            Some(Route::Logout)
        );
        assert_eq!(
            Route::parse(&Method::Post, "/auth/disconnect"),
            Some(Route::Disconnect)
        );
        assert_eq!(Route::parse(&Method::Get, "/api/me"), Some(Route::Me));
        for (method, path) in [
            (Method::Post, "/auth/login"),
            (Method::Get, "/auth/logout"),
            (Method::Get, "/auth/disconnect"),
            (Method::Delete, "/api/me"),
        ] {
            assert_eq!(
                Route::parse(&method, path),
                Some(Route::WrongMethod),
                "{path}"
            );
        }
    }

    #[test]
    fn other_paths_are_not_auth() {
        for path in [
            "/",
            "/signin",
            "/api/health",
            "/auth",
            "/auth/login/",
            "/api/me/x",
        ] {
            assert_eq!(Route::parse(&Method::Get, path), None, "{path}");
        }
    }
}
