use domain::email::{EmailHeaders, EmailTrigger};
use worker::{console_error, console_log, Env, ForwardableEmailMessage, Result};

use crate::auth::now;
use crate::db::email_state;
use crate::sync::{self, KickReason};

const FORWARD_TO: &str = "FORWARD_TO";
const NO_FORWARD_REASON: &str = "Radar has no forward address configured";
const FORWARD_FAILED_REASON: &str = "Radar could not forward this message";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Forward {
    Ok,
    Failed,
    Rejected,
}

impl Forward {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
        }
    }
}

fn log_line(trigger: Option<&EmailTrigger>, forward: Forward) -> String {
    let (reason, repo, sender, outcome) = match trigger {
        Some(t) => (
            t.reason.as_str().to_owned(),
            t.repo
                .as_ref()
                .map_or("none".to_owned(), ToString::to_string),
            t.sender
                .as_ref()
                .map_or("none".to_owned(), ToString::to_string),
            "kicked",
        ),
        None => (
            "none".to_owned(),
            "none".to_owned(),
            "none".to_owned(),
            "not_github",
        ),
    };
    format!(
        "email trigger reason={reason} repo={repo} sender={sender} forward={} outcome={outcome}",
        forward.as_str()
    )
}

fn forward_address(env: &Env) -> Option<String> {
    let value = env
        .secret(FORWARD_TO)
        .map(|s| s.to_string())
        .or_else(|_| env.var(FORWARD_TO).map(|v| v.to_string()))
        .ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

async fn record(env: &Env, trigger: &EmailTrigger) -> Result<()> {
    email_state::record(&env.d1("DB")?, now(), trigger.reason).await
}

// Only three headers are read. The message body is never opened.
#[worker::event(email)]
async fn email(message: ForwardableEmailMessage, env: Env, _ctx: worker::Context) -> Result<()> {
    let forward = match forward_address(&env) {
        Some(to) => match message.forward(&to).await {
            Ok(_) => Forward::Ok,
            Err(err) => {
                console_error!("email forward failed: {err:?}");
                message.set_reject(FORWARD_FAILED_REASON);
                Forward::Failed
            }
        },
        None => {
            message.set_reject(NO_FORWARD_REASON);
            Forward::Rejected
        }
    };

    let headers = message.headers();
    let reason = headers.get("x-github-reason").unwrap_or(None);
    let list_id = headers.get("list-id").unwrap_or(None);
    let sender = headers.get("x-github-sender").unwrap_or(None);
    let parsed = EmailTrigger::from_headers(&EmailHeaders {
        reason: reason.as_deref(),
        list_id: list_id.as_deref(),
        sender: sender.as_deref(),
    })
    .ok();

    console_log!("{}", log_line(parsed.as_ref(), forward));
    if let Some(trigger) = &parsed {
        if let Err(err) = sync::kick(&env, KickReason::Email).await {
            console_error!("syncer kick failed: {err}");
        }
        // The forward is done, so a D1 failure only costs the health clock.
        if let Err(err) = record(&env, trigger).await {
            console_error!("email state write failed: {err}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::email::EmailReason;
    use domain::ids::{Login, RepoName};

    #[test]
    fn log_line_for_a_github_email() {
        let trigger = EmailTrigger {
            reason: EmailReason::Mention,
            repo: RepoName::parse("octo/radar-demo").ok(),
            sender: Login::parse("maintainer").ok(),
        };
        assert_eq!(
            log_line(Some(&trigger), Forward::Ok),
            "email trigger reason=mention repo=octo/radar-demo sender=maintainer forward=ok outcome=kicked"
        );
    }

    #[test]
    fn log_line_for_missing_fields_and_other_mail() {
        let trigger = EmailTrigger {
            reason: EmailReason::TeamMention,
            repo: None,
            sender: None,
        };
        assert_eq!(
            log_line(Some(&trigger), Forward::Rejected),
            "email trigger reason=team_mention repo=none sender=none forward=rejected outcome=kicked"
        );
        assert_eq!(
            log_line(None, Forward::Failed),
            "email trigger reason=none repo=none sender=none forward=failed outcome=not_github"
        );
    }
}
