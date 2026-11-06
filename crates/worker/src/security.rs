use worker::{Headers, Response, ResponseBuilder, Result};

const CSP: &[(&str, &[&str])] = &[
    ("default-src", &["'none'"]),
    ("script-src", &["'self'", "'wasm-unsafe-eval'"]),
    ("style-src", &["'self'"]),
    ("font-src", &["'self'"]),
    ("img-src", &["'self'"]),
    ("connect-src", &["'self'"]),
    ("manifest-src", &["'self'"]),
    ("base-uri", &["'none'"]),
    ("form-action", &["'self'"]),
    ("frame-ancestors", &["'none'"]),
    ("object-src", &["'none'"]),
];

const PLAIN: &[(&str, &str)] = &[
    ("referrer-policy", "no-referrer"),
    ("x-content-type-options", "nosniff"),
    (
        "strict-transport-security",
        "max-age=31536000; includeSubDomains",
    ),
];

fn csp() -> String {
    CSP.iter()
        .map(|(directive, sources)| format!("{directive} {}", sources.join(" ")))
        .collect::<Vec<_>>()
        .join("; ")
}

fn headers() -> Vec<(&'static str, String)> {
    let mut all = vec![("content-security-policy", csp())];
    all.extend(
        PLAIN
            .iter()
            .map(|(name, value)| (*name, (*value).to_string())),
    );
    all
}

fn secured(upstream: Vec<(String, String)>) -> Vec<(String, String)> {
    let ours = headers();
    let mut merged: Vec<_> = upstream
        .into_iter()
        .filter(|(name, _)| !ours.iter().any(|(own, _)| name.eq_ignore_ascii_case(own)))
        .collect();
    merged.extend(
        ours.into_iter()
            .map(|(name, value)| (name.to_string(), value)),
    );
    merged
}

// Responses from ASSETS have immutable headers, so the response is rebuilt.
pub fn apply(response: Response) -> Result<Response> {
    let status = response.status_code();
    let headers = Headers::new();
    for (name, value) in secured(response.headers().entries().collect()) {
        headers.append(&name, &value)?;
    }
    let (_, body) = response.into_parts();
    Ok(ResponseBuilder::new()
        .with_status(status)
        .with_headers(headers)
        .body(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value_of<'a>(list: &'a [(String, String)], name: &str) -> Vec<&'a str> {
        list.iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect()
    }

    #[test]
    fn csp_denies_by_default() {
        let policy = csp();
        assert!(policy.starts_with("default-src 'none'; "));
        for closed in [
            "base-uri 'none'",
            "object-src 'none'",
            "frame-ancestors 'none'",
        ] {
            assert!(policy.contains(closed), "{closed}");
        }
    }

    #[test]
    fn csp_never_allows_unsafe_inline_or_eval() {
        for (directive, sources) in CSP {
            for source in *sources {
                assert!(
                    !matches!(*source, "'unsafe-inline'" | "'unsafe-eval'" | "*" | "data:"),
                    "{directive} allows {source}"
                );
            }
        }
        assert_eq!(
            CSP.iter()
                .find(|(d, _)| *d == "script-src")
                .map(|(_, s)| *s),
            Some(&["'self'", "'wasm-unsafe-eval'"][..])
        );
    }

    #[test]
    fn all_four_headers_are_set() {
        let set = secured(Vec::new());
        assert_eq!(set.len(), 4);
        assert_eq!(value_of(&set, "content-security-policy"), [csp().as_str()]);
        assert_eq!(value_of(&set, "referrer-policy"), ["no-referrer"]);
        assert_eq!(value_of(&set, "x-content-type-options"), ["nosniff"]);
        assert_eq!(
            value_of(&set, "strict-transport-security"),
            ["max-age=31536000; includeSubDomains"]
        );
    }

    #[test]
    fn a_weaker_upstream_csp_is_overwritten() {
        let set = secured(vec![
            ("Content-Security-Policy".into(), "default-src *".into()),
            ("content-type".into(), "text/html".into()),
        ]);
        assert_eq!(value_of(&set, "content-security-policy"), [csp().as_str()]);
        assert_eq!(value_of(&set, "content-type"), ["text/html"]);
    }
}
