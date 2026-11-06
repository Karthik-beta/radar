#![allow(
    dead_code,
    reason = "every field is parsed so the alarm pays the same validation cost as spikes/rust-fit/parse-bench"
)]

use serde::Deserialize;

pub const FIXTURE: &[u8] = include_bytes!("../fixture.json");

#[derive(Deserialize)]
struct Response {
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    search: Search,
}

#[derive(Deserialize)]
struct Search {
    nodes: Vec<Pr>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Actor {
    login: String,
    #[serde(rename = "__typename")]
    typename: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repo {
    name_with_owner: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Review {
    author: Option<Actor>,
    state: String,
    submitted_at: String,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rollup {
    state: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize)]
struct CommitNode {
    commit: Commit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TimelineItem {
    #[serde(rename = "__typename")]
    typename: String,
    created_at: Option<String>,
    actor: Option<Actor>,
    author: Option<Actor>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pr {
    id: String,
    number: u32,
    title: String,
    url: String,
    is_draft: bool,
    state: String,
    review_decision: Option<String>,
    created_at: String,
    updated_at: String,
    merged_at: Option<String>,
    closed_at: Option<String>,
    author: Option<Actor>,
    repository: Repo,
    reviews: Nodes<Review>,
    commits: Nodes<CommitNode>,
    timeline_items: Nodes<TimelineItem>,
}

impl Pr {
    fn failing(&self) -> bool {
        self.commits.nodes.iter().any(|c| {
            c.commit
                .status_check_rollup
                .as_ref()
                .is_some_and(|r| r.state == "FAILURE")
        })
    }
}

#[derive(Debug, PartialEq)]
pub struct Counts {
    pub parsed: i64,
    pub failing: i64,
    pub drafts: i64,
}

pub fn count(bytes: &[u8]) -> serde_json::Result<Counts> {
    let prs = serde_json::from_slice::<Response>(bytes)?.data.search.nodes;
    let tally = |keep: fn(&Pr) -> bool| prs.iter().filter(|p| keep(p)).count() as i64;
    Ok(Counts {
        parsed: prs.len() as i64,
        failing: tally(Pr::failing),
        drafts: tally(|p| p.is_draft),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Expected {
        prs: i64,
        failing: i64,
        drafts: i64,
    }

    #[test]
    fn embedded_fixture_matches_expected_json() {
        let expected: Expected =
            serde_json::from_str(include_str!("../expected.json")).expect("expected.json parses");
        assert_eq!(
            count(FIXTURE).expect("fixture parses"),
            Counts {
                parsed: expected.prs,
                failing: expected.failing,
                drafts: expected.drafts,
            }
        );
    }
}
