//! The shape of the `Enrich` response. Each struct holds what the query selects
//! and nothing else, so an unknown field, such as a comment body, is dropped
//! where it is parsed (N5). A missing required field fails the whole response.
//!
//! GitHub tags an item and a timeline node by `__typename`. The wire structs
//! `RawItem` and `RawTimeline` hold the union of the fields, and a `TryFrom`
//! turns each into its typed variant. A serde internally tagged enum would
//! buffer every node and compile each field type twice, which costs about 30 KB
//! of the Worker bundle.

use std::fmt;

use domain::ids::Timestamp;
use serde::de::{self, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

/// `a0` to `a<n-1>` in query order, then `rateLimit`.
#[derive(Debug, PartialEq)]
pub struct EnrichData {
    pub aliases: Vec<(usize, Option<RepoNode>)>,
    pub rate_limit: RateLimit,
}

impl<'de> Deserialize<'de> for EnrichData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct DataVisitor;

        impl<'de> Visitor<'de> for DataVisitor {
            type Value = EnrichData;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("the Enrich data object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<EnrichData, A::Error> {
                let mut aliases = Vec::new();
                let mut rate_limit = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "rateLimit" {
                        rate_limit = Some(map.next_value()?);
                    } else if let Some(index) = alias_index(&key) {
                        aliases.push((index, map.next_value::<Option<RepoNode>>()?));
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(EnrichData {
                    aliases,
                    rate_limit: rate_limit.ok_or_else(|| de::Error::missing_field("rateLimit"))?,
                })
            }
        }

        deserializer.deserialize_map(DataVisitor)
    }
}

fn alias_index(key: &str) -> Option<usize> {
    let digits = key.strip_prefix('a')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn need<T>(value: Option<T>, field: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("missing field `{field}`"))
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct RepoNode {
    #[serde(rename = "issueOrPullRequest")]
    pub item: Option<ItemNode>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct RateLimit {
    pub cost: u32,
    pub remaining: u32,
    #[serde(rename = "resetAt")]
    pub reset_at: Timestamp,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct ActorNode {
    pub login: String,
    #[serde(rename = "__typename")]
    pub typename: String,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct LoginNode {
    pub login: String,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct Nodes<T> {
    pub nodes: Vec<Option<T>>,
}

impl<T> Nodes<T> {
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.nodes.iter().flatten()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Decision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewStateNode {
    Approved,
    ChangesRequested,
    Commented,
    Dismissed,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollupState {
    Expected,
    Error,
    Failure,
    Pending,
    Success,
}

/// A user, or a team that has no login.
#[derive(Debug, PartialEq, Deserialize)]
pub struct Reviewer {
    #[serde(rename = "__typename")]
    pub typename: String,
    pub login: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct ReviewRequestNode {
    #[serde(rename = "requestedReviewer")]
    pub reviewer: Option<Reviewer>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct LatestReview {
    pub id: String,
    pub state: ReviewStateNode,
    pub at: Timestamp,
    pub url: String,
    pub author: Option<ActorNode>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct LastCommit {
    pub commit: CommitHead,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct CommitHead {
    pub oid: String,
    #[serde(rename = "statusCheckRollup")]
    pub rollup: Option<Rollup>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct Rollup {
    pub state: RollupState,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct TimelineCommit {
    pub oid: String,
    pub at: Timestamp,
    pub author: Option<CommitAuthor>,
}

#[derive(Debug, PartialEq, Deserialize)]
pub struct CommitAuthor {
    pub user: Option<LoginNode>,
}

#[derive(Debug, PartialEq)]
pub struct IssueNode {
    pub id: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: IssueState,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub closed_at: Option<Timestamp>,
    pub author: Option<ActorNode>,
    pub assignees: Nodes<LoginNode>,
    pub timeline: Nodes<TimelineNode>,
}

#[derive(Debug, PartialEq)]
pub struct PrNode {
    pub id: String,
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub is_draft: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub closed_at: Option<Timestamp>,
    pub merged_at: Option<Timestamp>,
    pub head_ref: String,
    pub review_decision: Option<Decision>,
    pub author: Option<ActorNode>,
    pub assignees: Nodes<LoginNode>,
    pub review_requests: Nodes<ReviewRequestNode>,
    pub latest_reviews: Nodes<LatestReview>,
    pub commits: Nodes<LastCommit>,
    pub timeline: Nodes<TimelineNode>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawItem")]
pub enum ItemNode {
    PullRequest(Box<PrNode>),
    Issue(Box<IssueNode>),
}

impl ItemNode {
    pub fn id(&self) -> &str {
        match self {
            Self::PullRequest(pr) => &pr.id,
            Self::Issue(issue) => &issue.id,
        }
    }
}

#[derive(Deserialize)]
struct RawItem {
    #[serde(rename = "__typename")]
    typename: String,
    id: String,
    number: u32,
    title: String,
    url: String,
    #[serde(rename = "issueState")]
    issue_state: Option<IssueState>,
    #[serde(rename = "prState")]
    pr_state: Option<PrState>,
    #[serde(rename = "isDraft")]
    is_draft: Option<bool>,
    #[serde(rename = "createdAt")]
    created_at: Timestamp,
    #[serde(rename = "updatedAt")]
    updated_at: Timestamp,
    #[serde(rename = "closedAt")]
    closed_at: Option<Timestamp>,
    #[serde(rename = "mergedAt")]
    merged_at: Option<Timestamp>,
    #[serde(rename = "headRefName")]
    head_ref: Option<String>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<Decision>,
    author: Option<ActorNode>,
    assignees: Nodes<LoginNode>,
    #[serde(rename = "reviewRequests")]
    review_requests: Option<Nodes<ReviewRequestNode>>,
    #[serde(rename = "latestReviews")]
    latest_reviews: Option<Nodes<LatestReview>>,
    commits: Option<Nodes<LastCommit>>,
    #[serde(rename = "timelineItems")]
    timeline: Nodes<TimelineNode>,
}

impl TryFrom<RawItem> for ItemNode {
    type Error = String;

    fn try_from(raw: RawItem) -> Result<Self, String> {
        match raw.typename.as_str() {
            "PullRequest" => Ok(Self::PullRequest(Box::new(PrNode {
                state: need(raw.pr_state, "prState")?,
                is_draft: need(raw.is_draft, "isDraft")?,
                head_ref: need(raw.head_ref, "headRefName")?,
                review_requests: need(raw.review_requests, "reviewRequests")?,
                latest_reviews: need(raw.latest_reviews, "latestReviews")?,
                commits: need(raw.commits, "commits")?,
                id: raw.id,
                number: raw.number,
                title: raw.title,
                url: raw.url,
                created_at: raw.created_at,
                updated_at: raw.updated_at,
                closed_at: raw.closed_at,
                merged_at: raw.merged_at,
                review_decision: raw.review_decision,
                author: raw.author,
                assignees: raw.assignees,
                timeline: raw.timeline,
            }))),
            "Issue" => Ok(Self::Issue(Box::new(IssueNode {
                state: need(raw.issue_state, "issueState")?,
                id: raw.id,
                number: raw.number,
                title: raw.title,
                url: raw.url,
                created_at: raw.created_at,
                updated_at: raw.updated_at,
                closed_at: raw.closed_at,
                author: raw.author,
                assignees: raw.assignees,
                timeline: raw.timeline,
            }))),
            other => Err(format!("unexpected item type `{other}`")),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct CommentNode {
    pub id: String,
    pub at: Timestamp,
    pub url: String,
    pub actor: Option<ActorNode>,
}

#[derive(Debug, PartialEq)]
pub struct ReviewNode {
    pub id: String,
    pub at: Timestamp,
    pub url: String,
    pub state: ReviewStateNode,
    pub actor: Option<ActorNode>,
}

#[derive(Debug, PartialEq)]
pub struct ReviewRequestedNode {
    pub id: String,
    pub at: Timestamp,
    pub actor: Option<ActorNode>,
    pub reviewer: Option<Reviewer>,
}

#[derive(Debug, PartialEq)]
pub struct AssignedNode {
    pub id: String,
    pub at: Timestamp,
    pub actor: Option<ActorNode>,
    pub assignee: Option<Reviewer>,
}

#[derive(Debug, PartialEq)]
pub struct CommitNode {
    pub id: String,
    pub url: String,
    pub commit: TimelineCommit,
}

#[derive(Debug, PartialEq)]
pub struct EventNode {
    pub id: String,
    pub at: Timestamp,
    pub actor: Option<ActorNode>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(try_from = "RawTimeline")]
pub enum TimelineNode {
    IssueComment(CommentNode),
    PullRequestReview(ReviewNode),
    ReviewRequestedEvent(ReviewRequestedNode),
    AssignedEvent(AssignedNode),
    PullRequestCommit(CommitNode),
    MergedEvent(EventNode),
    ClosedEvent(EventNode),
    ReopenedEvent(EventNode),
    /// A type the query did not ask for. It is skipped.
    Other,
}

#[derive(Deserialize)]
struct RawTimeline {
    #[serde(rename = "__typename")]
    typename: String,
    id: Option<String>,
    at: Option<Timestamp>,
    url: Option<String>,
    state: Option<ReviewStateNode>,
    actor: Option<ActorNode>,
    #[serde(rename = "requestedReviewer")]
    requested_reviewer: Option<Reviewer>,
    assignee: Option<Reviewer>,
    commit: Option<TimelineCommit>,
}

impl TryFrom<RawTimeline> for TimelineNode {
    type Error = String;

    fn try_from(raw: RawTimeline) -> Result<Self, String> {
        let event = |raw: RawTimeline| -> Result<EventNode, String> {
            Ok(EventNode {
                id: need(raw.id, "id")?,
                at: need(raw.at, "at")?,
                actor: raw.actor,
            })
        };
        Ok(match raw.typename.as_str() {
            "IssueComment" => Self::IssueComment(CommentNode {
                id: need(raw.id, "id")?,
                at: need(raw.at, "at")?,
                url: need(raw.url, "url")?,
                actor: raw.actor,
            }),
            "PullRequestReview" => Self::PullRequestReview(ReviewNode {
                id: need(raw.id, "id")?,
                at: need(raw.at, "at")?,
                url: need(raw.url, "url")?,
                state: need(raw.state, "state")?,
                actor: raw.actor,
            }),
            "ReviewRequestedEvent" => Self::ReviewRequestedEvent(ReviewRequestedNode {
                id: need(raw.id, "id")?,
                at: need(raw.at, "at")?,
                actor: raw.actor,
                reviewer: raw.requested_reviewer,
            }),
            "AssignedEvent" => Self::AssignedEvent(AssignedNode {
                id: need(raw.id, "id")?,
                at: need(raw.at, "at")?,
                actor: raw.actor,
                assignee: raw.assignee,
            }),
            "PullRequestCommit" => Self::PullRequestCommit(CommitNode {
                id: need(raw.id, "id")?,
                url: need(raw.url, "url")?,
                commit: need(raw.commit, "commit")?,
            }),
            "MergedEvent" => Self::MergedEvent(event(raw)?),
            "ClosedEvent" => Self::ClosedEvent(event(raw)?),
            "ReopenedEvent" => Self::ReopenedEvent(event(raw)?),
            _ => Self::Other,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::github::client::interpret_query;
    use crate::github::{GithubError, Queried};

    const ENRICH_20: &str = include_str!("../../tests/fixtures/enrich_20.json");
    const ENRICH_EDGE: &str = include_str!("../../tests/fixtures/enrich_edge.json");
    const NOW: crate::github::Timestamp = crate::github::Timestamp(1_793_900_000);

    fn parse(body: &str) -> Result<Queried<EnrichData>, GithubError> {
        interpret_query(200, &|_: &str| None, body, NOW)
    }

    fn enriched(body: &str) -> EnrichData {
        parse(body).expect("response parses").data
    }

    fn item(data: &EnrichData, alias: usize) -> Option<&ItemNode> {
        data.aliases
            .iter()
            .find(|(i, _)| *i == alias)
            .and_then(|(_, repo)| repo.as_ref())
            .and_then(|repo| repo.item.as_ref())
    }

    #[test]
    fn twenty_items_parse_into_twenty_nodes() {
        let data = enriched(ENRICH_20);
        assert_eq!(data.aliases.len(), 20);
        for i in 0..20 {
            assert!(item(&data, i).is_some(), "a{i}");
        }
        let prs = data
            .aliases
            .iter()
            .filter(|(_, r)| {
                matches!(
                    r.as_ref().and_then(|r| r.item.as_ref()),
                    Some(ItemNode::PullRequest(_))
                )
            })
            .count();
        assert!((1..20).contains(&prs), "{prs} pull requests");
        assert!(data.rate_limit.remaining > 0);
    }

    #[test]
    fn the_edge_response_has_a_gone_item_a_ghost_and_a_bot() {
        let data = enriched(ENRICH_EDGE);
        let gone: Vec<usize> = data
            .aliases
            .iter()
            .filter(|(_, r)| r.as_ref().is_none_or(|r| r.item.is_none()))
            .map(|(i, _)| *i)
            .collect();
        assert_eq!(gone, [1], "the second alias is null");

        let ghosts = data
            .aliases
            .iter()
            .filter_map(|(_, r)| r.as_ref()?.item.as_ref())
            .filter(|node| match node {
                ItemNode::PullRequest(pr) => pr.author.is_none(),
                ItemNode::Issue(issue) => issue.author.is_none(),
            })
            .count();
        assert_eq!(ghosts, 1);

        let bot_comments = data
            .aliases
            .iter()
            .filter_map(|(_, r)| r.as_ref()?.item.as_ref())
            .flat_map(|node| match node {
                ItemNode::PullRequest(pr) => pr.timeline.iter().collect::<Vec<_>>(),
                ItemNode::Issue(issue) => issue.timeline.iter().collect::<Vec<_>>(),
            })
            .filter(|t| {
                matches!(t, TimelineNode::IssueComment(c)
                if c.actor.as_ref().is_some_and(|a| a.typename == "Bot"))
            })
            .count();
        assert_eq!(bot_comments, 1);
    }

    fn without(field: &str) -> GithubError {
        let mut value: serde_json::Value = serde_json::from_str(ENRICH_20).expect("json");
        let object = value["data"]["a0"]["issueOrPullRequest"]
            .as_object_mut()
            .expect("object");
        assert!(object.remove(field).is_some(), "{field} is in the fixture");
        parse(&value.to_string())
            .map(|q| q.data)
            .expect_err("must fail")
    }

    #[test]
    fn a_missing_required_field_is_an_error() {
        let parse_error = GithubError::Parse {
            path: "/graphql".into(),
        };
        // a0 is a pull request in the fixture, so these are its required fields.
        for field in [
            "prState",
            "isDraft",
            "headRefName",
            "reviewRequests",
            "latestReviews",
            "commits",
            "timelineItems",
            "id",
            "updatedAt",
        ] {
            assert_eq!(without(field), parse_error, "{field}");
        }
    }

    #[test]
    fn a_timeline_node_missing_a_required_field_is_an_error() {
        let mut value: serde_json::Value = serde_json::from_str(ENRICH_20).expect("json");
        let nodes = value["data"]["a4"]["issueOrPullRequest"]["timelineItems"]["nodes"]
            .as_array_mut()
            .expect("timeline");
        let comment = nodes
            .iter_mut()
            .find(|n| n["__typename"] == "IssueComment")
            .expect("a comment");
        comment.as_object_mut().expect("object").remove("url");
        assert!(parse(&value.to_string()).is_err());
    }

    #[test]
    fn a_response_without_rate_limit_is_an_error() {
        let body = json!({"data": {"a0": null}}).to_string();
        assert!(parse(&body).is_err());
    }

    #[test]
    fn an_unknown_item_type_is_an_error() {
        let body = json!({"data": {
            "a0": {"issueOrPullRequest": {"__typename": "Discussion", "id": "D_1", "number": 1,
                "title": "t", "url": "u", "createdAt": "2026-11-01T00:00:00Z",
                "updatedAt": "2026-11-01T00:00:00Z", "assignees": {"nodes": []},
                "timelineItems": {"nodes": []}}},
            "rateLimit": {"cost": 1, "remaining": 4999, "resetAt": "2026-11-07T10:00:00Z"}
        }})
        .to_string();
        assert!(parse(&body).is_err());
    }

    #[test]
    fn a_comment_body_in_the_response_is_dropped() {
        let body = json!({"data": {
            "a0": {"issueOrPullRequest": {
                "__typename": "Issue", "id": "I_1", "number": 1, "title": "t", "url": "u",
                "issueState": "OPEN", "createdAt": "2026-11-01T00:00:00Z",
                "updatedAt": "2026-11-01T00:00:00Z", "closedAt": null, "author": null,
                "body": "BODYMARK-7f3a",
                "assignees": {"nodes": []},
                "timelineItems": {"nodes": [{
                    "__typename": "IssueComment", "id": "IC_1", "at": "2026-11-01T00:00:00Z",
                    "url": "u", "actor": null, "body": "BODYMARK-7f3a", "bodyText": "BODYMARK-7f3a"
                }]}
            }},
            "rateLimit": {"cost": 1, "remaining": 4999, "resetAt": "2026-11-07T10:00:00Z"}
        }})
        .to_string();
        let data = enriched(&body);
        assert!(!format!("{data:?}").contains("BODYMARK"));
    }

    #[test]
    fn an_unlisted_timeline_type_is_skipped_not_fatal() {
        let body = json!({"data": {
            "a0": {"issueOrPullRequest": {
                "__typename": "Issue", "id": "I_1", "number": 1, "title": "t", "url": "u",
                "issueState": "CLOSED", "createdAt": "2026-11-01T00:00:00Z",
                "updatedAt": "2026-11-01T00:00:00Z", "closedAt": "2026-11-02T00:00:00Z",
                "author": null, "assignees": {"nodes": [null]},
                "timelineItems": {"nodes": [{"__typename": "LabeledEvent"}]}
            }},
            "rateLimit": {"cost": 1, "remaining": 4999, "resetAt": "2026-11-07T10:00:00Z"}
        }})
        .to_string();
        let data = enriched(&body);
        match item(&data, 0) {
            Some(ItemNode::Issue(issue)) => {
                assert_eq!(issue.timeline.nodes, vec![Some(TimelineNode::Other)])
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn alias_keys_are_a_letter_and_digits() {
        assert_eq!(alias_index("a0"), Some(0));
        assert_eq!(alias_index("a19"), Some(19));
        for bad in ["a", "rateLimit", "b1", "a-1", "a1x", ""] {
            assert_eq!(alias_index(bad), None, "{bad:?}");
        }
    }
}
