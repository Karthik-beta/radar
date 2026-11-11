use domain::repo_stats::RepoCard;
use worker::{console_error, Env, Result};

use super::Reply;
use crate::db::repo_stats;

/// The repo cards, read from D1 only (N7). A row that does not parse is
/// logged and left out.
pub async fn read(env: &Env) -> Result<Vec<RepoCard>> {
    let (cards, errors) = repo_stats::read(&env.d1("DB")?).await?.parse();
    for err in errors {
        console_error!("api repos skipped a row: {err}");
    }
    Ok(cards)
}

pub async fn respond(env: &Env) -> Result<Reply> {
    Ok(Reply::json(200, &read(env).await?))
}
