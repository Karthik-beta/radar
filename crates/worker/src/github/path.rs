use super::error::GithubError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiPath(String);

impl ApiPath {
    pub fn new(raw: &str) -> Result<Self, GithubError> {
        let rest = raw.strip_prefix('/').ok_or(GithubError::BadConfig)?;
        if rest.starts_with('/') || raw.contains("..") || raw.contains("://") {
            return Err(GithubError::BadConfig);
        }
        if raw
            .chars()
            .any(|c| c == '@' || c == '\\' || c == ' ' || c.is_control())
        {
            return Err(GithubError::BadConfig);
        }
        let lower = raw.to_ascii_lowercase();
        if ["%2e", "%2f", "%5c"].iter().any(|enc| lower.contains(enc)) {
            return Err(GithubError::BadConfig);
        }
        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_paths() {
        for ok in [
            "/user",
            "/rate_limit",
            "/notifications?all=false&since=2026-11-01T00:00:00Z",
            "/repos/Karthik-beta/radar/pulls/5",
        ] {
            let path = ApiPath::new(ok).expect(ok);
            assert_eq!(path.as_str(), ok);
        }
    }

    #[test]
    fn rejects_double_slash_dotdot_scheme_at_sign_and_backslash() {
        for bad in [
            "//evil.example/x",
            "@evil.example/x",
            "/../x",
            "/a/../x",
            "https://evil.example",
            "/x?u=https://evil.example",
            "/a%5Cb",
            "/a\\b",
            "/a@b",
            "/a\nb",
            "/a\0b",
            "/a b",
            "/%2e%2E/x",
            "/a%2Fb",
            "user",
            "",
        ] {
            assert_eq!(ApiPath::new(bad), Err(GithubError::BadConfig), "{bad:?}");
        }
    }
}
