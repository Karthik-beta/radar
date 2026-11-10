use domain::error::ErrorCode;
use domain::feed::{split_page, FeedPage, FeedQuery};
use worker::url::form_urlencoded;
use worker::{console_error, console_log, Env, Request, Result};

use super::Reply;
use crate::db::feed::{self, RawFeed};

/// The feed's query, parsed once at the boundary. It reads the raw URL string,
/// because `Request::url` runs the full `Url` parser and its IDNA tables.
fn query_of(req: &Request) -> Result<FeedQuery, domain::feed::FeedError> {
    let url = req.inner().url();
    let raw = url.split('#').next().and_then(|u| u.split_once('?'));
    let pairs: Vec<(String, String)> =
        form_urlencoded::parse(raw.map_or("", |(_, q)| q).as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
    FeedQuery::parse(pairs.iter().map(|(n, v)| (n.as_str(), v.as_str())))
}

pub async fn respond(req: &Request, env: &Env) -> Result<Reply> {
    let query = match query_of(req) {
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
