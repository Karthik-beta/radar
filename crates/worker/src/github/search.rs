//! The `Search` query behind the 90 day backfill and the nightly search. Each node
//! carries the `ItemFields` fragment the Enrich query uses, so a search result and
//! an enriched notification become the same row. No selection asks for a `body` (N5).

use serde::Deserialize;

use super::enrich_query::{item_fields, timeline_node};
use super::enrich_response::{ItemNode, RateLimit};
use super::query::QueryDoc;

/// The query comes first and the fragment after it, so the request body starts
/// with `query` (N2).
const SEARCH_TEXT: &str = concat!(
    "query Search($q: String!, $first: Int!, $after: String) { ",
    "search(type: ISSUE, first: $first, after: $after, query: $q) { ",
    "nodes { __typename ...ItemFields } pageInfo { endCursor hasNextPage } } ",
    "rateLimit { cost remaining resetAt } } ",
    item_fields!()
);

pub static SEARCH_QUERY: QueryDoc = QueryDoc::from_file("Search", SEARCH_TEXT);

#[derive(Debug, PartialEq, Deserialize)]
pub struct SearchData {
    pub search: SearchPage,
    #[serde(rename = "rateLimit")]
    pub rate_limit: RateLimit,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct SearchPage {
    pub nodes: Vec<Option<ItemNode>>,
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::query::validate_document;

    #[test]
    fn the_document_is_one_query_that_starts_with_query() {
        let text = SEARCH_QUERY.text();
        assert!(text.starts_with("query Search("));
        assert_eq!(validate_document(text), Ok(()));
        assert_eq!(SEARCH_QUERY.name(), "Search");
        for word in ["mutation", "subscription"] {
            assert!(!text.contains(word), "{word}");
        }
    }

    #[test]
    fn no_selection_asks_for_a_body() {
        assert!(!SEARCH_QUERY.text().to_ascii_lowercase().contains("body"));
    }

    #[test]
    fn it_shares_the_enrich_fragment_once() {
        let text = SEARCH_QUERY.text();
        assert_eq!(text.matches("fragment ItemFields").count(), 1);
        assert!(text.ends_with(crate::github::enrich_query::ENRICH_FRAGMENT));
        assert!(text.contains("$first: Int!"));
        assert!(text.contains("pageInfo { endCursor hasNextPage }"));
    }
}
