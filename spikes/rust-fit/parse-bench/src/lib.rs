use serde::Deserialize;

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

fn score(prs: &[Pr]) -> u32 {
    prs.iter()
        .map(|p| {
            let last_human = p
                .timeline_items
                .nodes
                .iter()
                .rev()
                .filter_map(|t| t.actor.as_ref().or(t.author.as_ref()).map(|a| (t, a)))
                .find(|(_, a)| a.typename != "Bot");
            let failing = p.commits.nodes.iter().any(|c| c.commit.status_check_rollup.as_ref().is_some_and(|r| r.state == "FAILURE"));
            let changes = p.reviews.nodes.iter().any(|r| r.state == "CHANGES_REQUESTED" && r.author.is_some() && !r.submitted_at.is_empty());
            let mine = last_human.is_some_and(|(t, a)| a.login == "karthik" && t.created_at.is_some() && !t.typename.is_empty());
            (failing as u32)
                + (changes as u32)
                + (mine as u32)
                + (p.is_draft as u32)
                + (p.review_decision.is_some() as u32)
                + (p.merged_at.is_some() as u32)
                + (p.closed_at.is_some() as u32)
                + (p.number % 2)
                + (p.id.len() + p.url.len() + p.title.len() + p.state.len() + p.created_at.len() + p.updated_at.len() + p.repository.name_with_owner.len()) as u32 % 2
                + p.author.as_ref().map_or(0, |a| a.login.len() as u32 % 2)
        })
        .sum()
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len));
}

#[no_mangle]
pub unsafe extern "C" fn parse(ptr: *const u8, len: usize) -> u32 {
    let bytes = std::slice::from_raw_parts(ptr, len);
    match serde_json::from_slice::<Response>(bytes) {
        Ok(r) => score(&r.data.search.nodes) + 1,
        Err(_) => 0,
    }
}
