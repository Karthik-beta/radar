//! The batched query that enriches notification threads. A notification names
//! an item by repository and number, so each alias addresses one item that way.
//! No selection asks for a `body` (N5).

use std::fmt::Write as _;

use serde_json::{json, Map, Value};

use super::query::QueryDoc;

macro_rules! timeline_node {
    (comment) => {
        "... on IssueComment { id at: createdAt url actor: author { login __typename } }"
    };
    (review) => {
        "... on PullRequestReview { id at: createdAt url state actor: author { login __typename } }"
    };
    (review_requested) => {
        "... on ReviewRequestedEvent { id at: createdAt actor { login __typename } \
         requestedReviewer { __typename ... on User { login } } }"
    };
    (assigned) => {
        "... on AssignedEvent { id at: createdAt actor { login __typename } \
         assignee { __typename ... on User { login } ... on Bot { login } } }"
    };
    (commit) => {
        "... on PullRequestCommit { id url commit { oid at: committedDate author { user { login } } } }"
    };
    (merged) => {
        "... on MergedEvent { id at: createdAt actor { login __typename } }"
    };
    (closed) => {
        "... on ClosedEvent { id at: createdAt actor { login __typename } }"
    };
    (reopened) => {
        "... on ReopenedEvent { id at: createdAt actor { login __typename } }"
    };
}

pub(crate) use timeline_node;

/// The fields of one pull request or issue. `Issue` and `PullRequest` each take
/// the timeline types their own union allows, and each names its `state` apart
/// because the two state enums are different types. A macro, because `concat!`
/// takes only literals and the Search query appends the same fragment.
macro_rules! item_fields {
    () => {
        concat!(
    "fragment ItemFields on IssueOrPullRequest { ",
    "... on Issue { id number title url issueState: state createdAt updatedAt closedAt ",
    "author { login __typename } assignees(first: 10) { nodes { login } } ",
    "timelineItems(last: 50, itemTypes: [ISSUE_COMMENT, ASSIGNED_EVENT, CLOSED_EVENT, \
     REOPENED_EVENT]) { nodes { __typename ",
    timeline_node!(comment),
    " ",
    timeline_node!(assigned),
    " ",
    timeline_node!(closed),
    " ",
    timeline_node!(reopened),
    " } } } ",
    "... on PullRequest { id number title url prState: state isDraft createdAt updatedAt ",
    "closedAt mergedAt headRefName reviewDecision author { login __typename } ",
    "assignees(first: 10) { nodes { login } } ",
    "reviewRequests(first: 10) { nodes { requestedReviewer { __typename ... on User { login } } } } ",
    "latestReviews(first: 10) { nodes { id state at: createdAt url author { login __typename } } } ",
    "commits(last: 1) { nodes { commit { oid statusCheckRollup { state } } } } ",
    "timelineItems(last: 50, itemTypes: [ISSUE_COMMENT, PULL_REQUEST_REVIEW, \
     REVIEW_REQUESTED_EVENT, ASSIGNED_EVENT, PULL_REQUEST_COMMIT, MERGED_EVENT, CLOSED_EVENT, \
     REOPENED_EVENT]) { nodes { __typename ",
    timeline_node!(comment),
    " ",
    timeline_node!(review),
    " ",
    timeline_node!(review_requested),
    " ",
    timeline_node!(assigned),
    " ",
    timeline_node!(commit),
    " ",
    timeline_node!(merged),
    " ",
    timeline_node!(closed),
    " ",
    timeline_node!(reopened),
    " } } } }"
        )
    };
}

pub(crate) use item_fields;

pub const ENRICH_FRAGMENT: &str = item_fields!();

/// One item to read: a repository and a number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub owner: String,
    pub name: String,
    pub number: u32,
}

/// The `Enrich` operation for `n` items. The query comes first and the fragment
/// after it, so the request body starts with `query` (N2).
pub fn build_enrich_query(n: usize) -> QueryDoc {
    let mut text = String::with_capacity(256 + n * 160 + ENRICH_FRAGMENT.len());
    text.push_str("query Enrich(");
    for i in 0..n {
        let sep = if i == 0 { "" } else { ", " };
        let _ = write!(text, "{sep}$o{i}: String!, $n{i}: String!, $i{i}: Int!");
    }
    text.push_str(") { ");
    for i in 0..n {
        let _ = write!(
            text,
            "a{i}: repository(owner: $o{i}, name: $n{i}) {{ \
             issueOrPullRequest(number: $i{i}) {{ __typename ...ItemFields }} }} "
        );
    }
    text.push_str("rateLimit { cost remaining resetAt } } ");
    text.push_str(ENRICH_FRAGMENT);
    QueryDoc::built("Enrich", text)
}

/// The variables for `build_enrich_query(targets.len())`.
pub fn enrich_variables(targets: &[Target]) -> Value {
    let mut vars = Map::new();
    for (i, target) in targets.iter().enumerate() {
        vars.insert(format!("o{i}"), json!(target.owner));
        vars.insert(format!("n{i}"), json!(target.name));
        vars.insert(format!("i{i}"), json!(target.number));
    }
    Value::Object(vars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::query::validate_document;

    #[test]
    fn documents_are_queries_that_start_with_query() {
        for n in [0, 1, 10, 20] {
            let doc = build_enrich_query(n);
            assert!(doc.text().starts_with("query Enrich("), "n={n}");
            assert_eq!(validate_document(doc.text()), Ok(()), "n={n}");
            assert!(!doc.text().contains("mutation"), "n={n}");
            assert!(!doc.text().contains("subscription"), "n={n}");
        }
    }

    #[test]
    fn no_selection_asks_for_a_body() {
        let text = build_enrich_query(20).text().to_ascii_lowercase();
        assert!(!text.contains("body"));
        assert!(!ENRICH_FRAGMENT.to_ascii_lowercase().contains("body"));
    }

    #[test]
    fn one_alias_and_three_variables_per_item() {
        let doc = build_enrich_query(3);
        let text = doc.text();
        for i in 0..3 {
            assert!(text.contains(&format!("a{i}: repository(owner: $o{i}, name: $n{i})")));
            assert!(text.contains(&format!("$i{i}: Int!")));
        }
        assert!(!text.contains("a3:"));
        assert!(text.contains("rateLimit { cost remaining resetAt }"));
        assert_eq!(text.matches("fragment ItemFields").count(), 1);
    }

    #[test]
    fn variables_match_the_document() {
        let targets = [
            Target {
                owner: "octo".into(),
                name: "widgets".into(),
                number: 12,
            },
            Target {
                owner: "octo".into(),
                name: "gadgets".into(),
                number: 9,
            },
        ];
        let vars = enrich_variables(&targets);
        assert_eq!(vars["o0"], "octo");
        assert_eq!(vars["n1"], "gadgets");
        assert_eq!(vars["i0"], 12);
        assert_eq!(vars.as_object().map(Map::len), Some(6));
    }

    #[test]
    fn a_full_batch_fits_the_request_budget() {
        let doc = build_enrich_query(crate::sync::config::ENRICH_BATCH);
        assert!(doc.text().len() < 8 * 1024, "{} bytes", doc.text().len());
    }
}
