//! Reads D1 for `Freshness.email`. The Syncer builds the whole freshness body,
//! so this read sits beside the status read and the Worker keeps passing the
//! JSON through.

use std::sync::OnceLock;

use domain::email_health::{email_health, EmailHealth, QuietWindow};
use domain::ids::Timestamp;
use worker::{console_log, Env, Result};

use crate::db::email_state;

const QUIET_SECS_VAR: &str = "EMAIL_QUIET_SECS";

/// Parsed once per isolate, so a bad value logs once.
fn quiet_window(env: &Env) -> QuietWindow {
    static WINDOW: OnceLock<QuietWindow> = OnceLock::new();
    *WINDOW.get_or_init(|| {
        let raw = env.var(QUIET_SECS_VAR).ok().map(|v| v.to_string());
        let parsed = QuietWindow::parse(raw.as_deref());
        if parsed.ignored {
            console_log!("ignored {QUIET_SECS_VAR}, using the default window");
        }
        parsed.window
    })
}

pub async fn current(env: &Env, now: Timestamp) -> Result<EmailHealth> {
    let db = env.d1("DB")?;
    let last = email_state::read(&db).await?;
    let threads = email_state::threads_after(&db, last).await?;
    Ok(email_health(now, last, threads, quiet_window(env)))
}
