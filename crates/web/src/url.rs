/// A link Radar is willing to put in an `href`: an `https://github.com/` URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubUrl(String);

impl GithubUrl {
    pub fn parse(url: &str) -> Option<Self> {
        is_github_url(url).then(|| Self(url.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn is_github_url(url: &str) -> bool {
    url.starts_with("https://github.com/") && !url.chars().any(|c| c.is_whitespace() || c == '\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_accepts_only_https_github() {
        assert!(is_github_url(
            "https://github.com/o/r/pull/1#issuecomment-2"
        ));
        for bad in [
            "javascript:alert(1)",
            "http://github.com/x",
            "https://github.com.evil.example/x",
            "https://github.com",
            "https://github.com/a\\@evil.example",
            "https://github.com/ x",
            "",
        ] {
            assert!(!is_github_url(bad), "{bad}");
            assert_eq!(GithubUrl::parse(bad), None, "{bad}");
        }
    }
}
