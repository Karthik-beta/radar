use domain::agents::agents_page;
use domain::ids::Timestamp;
use worker::{console_error, Env, Result};

use super::Reply;
use crate::db::agents;

pub async fn respond(env: &Env, now: Timestamp) -> Result<Reply> {
    let raw = agents::read(&env.d1("DB")?, now).await?;
    let mut rows = Vec::with_capacity(raw.len());
    for row in raw {
        match row.parse() {
            Ok(row) => rows.push(row),
            Err(err) => console_error!("api agents skipped a row: {err}"),
        }
    }
    Ok(Reply::json(200, &agents_page(rows, now)))
}
