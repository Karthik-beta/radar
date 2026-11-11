use worker::Method;

/// The inbound `/hooks/` routes. Both receive a POST from outside Radar and
/// write only Radar's own D1 rows; nothing here sends a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    Github,
    Checkin,
    WrongMethod,
    NotFound,
}

impl Hook {
    /// `None` means the path is not under `/hooks`.
    pub fn parse(method: &Method, path: &str) -> Option<Self> {
        if path != "/hooks" && !path.starts_with("/hooks/") {
            return None;
        }
        let hook = match path {
            "/hooks/github" => Self::Github,
            "/hooks/checkin" => Self::Checkin,
            _ => return Some(Self::NotFound),
        };
        Some(if *method == Method::Post {
            hook
        } else {
            Self::WrongMethod
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_answer_only_post() {
        assert_eq!(
            Hook::parse(&Method::Post, "/hooks/github"),
            Some(Hook::Github)
        );
        assert_eq!(
            Hook::parse(&Method::Post, "/hooks/checkin"),
            Some(Hook::Checkin)
        );
        assert_eq!(
            Hook::parse(&Method::Get, "/hooks/github"),
            Some(Hook::WrongMethod)
        );
        assert_eq!(
            Hook::parse(&Method::Post, "/hooks/other"),
            Some(Hook::NotFound)
        );
        assert_eq!(Hook::parse(&Method::Post, "/hooks"), Some(Hook::NotFound));
        assert_eq!(Hook::parse(&Method::Post, "/hooksx"), None);
        assert_eq!(Hook::parse(&Method::Get, "/api/agents"), None);
    }
}
