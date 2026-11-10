use domain::error::ErrorCode;
use domain::feed::{split_page, FeedPage, FeedQuery};
use worker::{console_error, console_log, Env, Request, Result};

use super::Reply;
use crate::db::feed::{self, RawFeed};

/// The feed's query, parsed once at the boundary.
fn query_of(req: &Request) -> Result<std::result::Result<FeedQuery, domain::feed::FeedError>> {
    let url = req.url()?;
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    Ok(FeedQuery::parse(
        pairs.iter().map(|(n, v)| (n.as_str(), v.as_str())),
    ))
}

pub async fn respond(req: &Request, env: &Env) -> Result<Reply> {
    let query = match query_of(req)? {
        Ok(query) => query,
        Err(err) => {
            console_log!("api feed rejected reason={err}");
            return Ok(Reply::error(400, ErrorCode::BadRequest));
        }
    };
    let raw = feed::read(&env.d1("DB")?, &query).await?;
    let (page, errors) = assemble(raw);
    for err in errors {
        console_error!("api feed skipped a row: {err}");
    }
    Ok(Reply::json(200, &page))
}

/// Validates every row and drops the ones that do not parse, so one bad row
/// cannot blank the feed.
pub fn assemble(raw: RawFeed) -> (FeedPage, Vec<crate::db::RowError>) {
    let mut errors = Vec::new();
    let events = raw
        .rows
        .into_iter()
        .filter_map(|row| row.parse().map_err(|e| errors.push(e)).ok())
        .collect();
    let repos = raw
        .repos
        .into_iter()
        .filter_map(|repo| repo.parse().map_err(|e| errors.push(e)).ok())
        .collect();
    let (events, next) = split_page(events);
    (
        FeedPage {
            events,
            next,
            repos,
        },
        errors,
    )
}
