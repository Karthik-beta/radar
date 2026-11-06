use super::error::GithubError;

#[derive(Debug)]
pub struct QueryDoc {
    name: &'static str,
    text: &'static str,
}

impl QueryDoc {
    pub(crate) const fn from_file(name: &'static str, text: &'static str) -> Self {
        Self { name, text }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn text(&self) -> &'static str {
        self.text
    }
}

macro_rules! query_doc {
    ($name:literal, $file:literal) => {
        $crate::github::query::QueryDoc::from_file($name, include_str!(concat!("queries/", $file)))
    };
}

pub const VIEWER: QueryDoc = query_doc!("Viewer", "viewer.graphql");

pub const ALL: &[&QueryDoc] = &[&VIEWER];

pub fn find(name: &str) -> Option<&'static QueryDoc> {
    ALL.iter().copied().find(|doc| doc.name == name)
}

pub fn validate_document(doc: &str) -> Result<(), GithubError> {
    let bytes = doc.as_bytes();
    let mut i = usize::from(doc.starts_with('\u{feff}')) * 3;
    let mut open: Vec<u8> = Vec::new();
    let mut want_definition = true;
    let mut queries = 0usize;
    while let Some(&c) = bytes.get(i) {
        match c {
            b' ' | b'\t' | b'\n' | b'\r' | b',' => i += 1,
            b'#' => {
                while bytes.get(i).is_some_and(|&b| b != b'\n' && b != b'\r') {
                    i += 1;
                }
            }
            b'"' => i = skip_string(bytes, i)?,
            b'{' | b'(' | b'[' => {
                if open.is_empty() && want_definition {
                    want_definition = false;
                    if c == b'{' {
                        queries += 1;
                    } else {
                        return Err(GithubError::BadConfig);
                    }
                }
                open.push(c);
                i += 1;
            }
            b'}' | b')' | b']' => {
                let expected = match c {
                    b'}' => b'{',
                    b')' => b'(',
                    _ => b'[',
                };
                if open.pop() != Some(expected) {
                    return Err(GithubError::BadConfig);
                }
                if open.is_empty() && c == b'}' {
                    want_definition = true;
                }
                i += 1;
            }
            c if is_name_start(c) => {
                let start = i;
                while bytes.get(i).is_some_and(|&b| is_name_char(b)) {
                    i += 1;
                }
                if open.is_empty() && want_definition {
                    match doc.get(start..i) {
                        Some("query") => queries += 1,
                        Some("fragment") => {}
                        _ => return Err(GithubError::BadConfig),
                    }
                    want_definition = false;
                }
            }
            c if c.is_ascii_digit() || c == b'-' => {
                while bytes
                    .get(i)
                    .is_some_and(|&b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'))
                {
                    i += 1;
                }
            }
            b'$' | b'@' | b':' | b'=' | b'!' | b'|' | b'&' | b'.' => i += 1,
            _ => return Err(GithubError::BadConfig),
        }
    }
    if !open.is_empty() || queries == 0 {
        return Err(GithubError::BadConfig);
    }
    Ok(())
}

fn is_name_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

fn is_name_char(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

fn skip_string(bytes: &[u8], start: usize) -> Result<usize, GithubError> {
    let mut i = start;
    if bytes.get(i..i + 3) == Some(b"\"\"\"") {
        i += 3;
        while i < bytes.len() {
            if bytes.get(i..i + 4) == Some(b"\\\"\"\"") {
                i += 4;
            } else if bytes.get(i..i + 3) == Some(b"\"\"\"") {
                return Ok(i + 3);
            } else {
                i += 1;
            }
        }
        return Err(GithubError::BadConfig);
    }
    i += 1;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'"' => return Ok(i + 1),
            b'\\' => i += 2,
            b'\n' | b'\r' => return Err(GithubError::BadConfig),
            _ => i += 1,
        }
    }
    Err(GithubError::BadConfig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_mutation_subscription_and_mixed_documents() {
        for bad in [
            "mutation M { addStar(input: {}) { clientMutationId } }",
            "subscription S { x }",
            "query A { a } mutation B { b }",
            "mutation B { b } query A { a }",
            "query A { a } fragment F on User { b } subscription S { c }",
            "query A($t: String = \"x\") { a } mutation B { b }",
            "query A { a } # ok\nmutation B { b }",
            "query A { a(t: \"\\\" \") } mutation B { b }",
            "query { a } }",
            "query { a",
            "",
            "# nothing here",
            "fragment F on User { a }",
            "Query { a }",
            "{ a } mutation B { b }",
            "query A { a } \"stray\" mutation B { b }",
            "query A { a } \u{00e9}",
        ] {
            assert_eq!(
                validate_document(bad),
                Err(GithubError::BadConfig),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn accepts_queries_with_mutation_in_comments_strings_and_field_names() {
        for good in [
            "query A { mutationCount }",
            "query A { a } # mutation B { b }",
            "# mutation B { b }\nquery A { a }",
            "query A($t: String = \"mutation\") { a(t: $t) }",
            "query A { a(t: \"subscription \\\" mutation\") }",
            "query A { a(t: \"\"\"mutation M { x } \\\"\"\" mutation\"\"\") }",
            "{ viewer { login } }",
            "query A($o: In = {x: 1}) @dir { a(o: $o) }",
            "query A { a } query B { b }",
            "query A { a } fragment F on User { mutation }",
            "\u{feff}query A { a }",
            "query mutation { a }",
            "query A { a(n: -1.5e3, l: [1, 2]) }",
        ] {
            assert_eq!(validate_document(good), Ok(()), "{good:?}");
        }
    }

    #[test]
    fn every_registered_document_is_a_query() {
        assert!(!ALL.is_empty());
        for doc in ALL {
            assert_eq!(validate_document(doc.text()), Ok(()), "{}", doc.name());
            assert!(
                doc.text().contains(&format!("query {}", doc.name())),
                "{} names its own operation",
                doc.name()
            );
        }
    }

    #[test]
    fn find_looks_up_by_name() {
        assert_eq!(find("Viewer").map(QueryDoc::name), Some("Viewer"));
        assert!(find("Nope").is_none());
    }
}
