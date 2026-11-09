//! Whether GitHub notification emails still reach Radar (R19). Radar wakes its
//! sync on those emails, so a silent inbox means it falls back to the cron.

use serde::{Deserialize, Serialize};

use crate::ids::Timestamp;

pub const DEFAULT_QUIET_SECS: i64 = 24 * 60 * 60;

/// How long without a GitHub email counts as quiet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuietWindow(i64);

impl Default for QuietWindow {
    fn default() -> Self {
        Self(DEFAULT_QUIET_SECS)
    }
}

/// What `QuietWindow::parse` made of `EMAIL_QUIET_SECS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedWindow {
    pub window: QuietWindow,
    /// The var was set to something that is not a positive whole number.
    pub ignored: bool,
}

impl QuietWindow {
    pub fn secs(self) -> i64 {
        self.0
    }

    /// A missing var is the normal default. A set var that is not a positive
    /// whole number falls back to 24 hours and says so through `ignored`.
    pub fn parse(raw: Option<&str>) -> ParsedWindow {
        let Some(raw) = raw else {
            return ParsedWindow {
                window: Self::default(),
                ignored: false,
            };
        };
        match raw.trim().parse::<i64>() {
            Ok(secs) if secs > 0 => ParsedWindow {
                window: Self(secs),
                ignored: false,
            },
            _ => ParsedWindow {
                window: Self::default(),
                ignored: true,
            },
        }
    }
}

/// `Flowing` has an email inside the window. `Idle` is quiet with nothing new
/// on GitHub. `Silent` is quiet while threads changed, so mail is being lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum EmailHealth {
    Flowing {
        last_email_at: Timestamp,
    },
    Idle {
        last_email_at: Option<Timestamp>,
    },
    Silent {
        last_email_at: Option<Timestamp>,
        threads_missed: u32,
    },
}

/// `threads_after` counts notification threads that changed after the last
/// email, or all of them when no email has come. A `last_email_at` in the
/// future, from clock skew, counts as flowing.
pub fn email_health(
    now: Timestamp,
    last_email_at: Option<Timestamp>,
    threads_after: u32,
    window: QuietWindow,
) -> EmailHealth {
    if let Some(last) = last_email_at {
        if now.0.saturating_sub(last.0) < window.secs() {
            return EmailHealth::Flowing {
                last_email_at: last,
            };
        }
    }
    if threads_after > 0 {
        EmailHealth::Silent {
            last_email_at,
            threads_missed: threads_after,
        }
    } else {
        EmailHealth::Idle { last_email_at }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: Timestamp = Timestamp(1_793_000_000);
    const HOUR: i64 = 3600;

    fn ago(secs: i64) -> Option<Timestamp> {
        Some(Timestamp(NOW.0 - secs))
    }

    fn health(last: Option<Timestamp>, threads: u32) -> EmailHealth {
        email_health(NOW, last, threads, QuietWindow::default())
    }

    #[test]
    fn email_health_is_flowing_inside_the_window_even_with_a_newer_thread() {
        assert_eq!(
            health(ago(HOUR), 1),
            EmailHealth::Flowing {
                last_email_at: Timestamp(NOW.0 - HOUR)
            }
        );
    }

    #[test]
    fn email_health_past_the_window_is_silent_with_a_newer_thread_and_idle_without() {
        assert_eq!(
            health(ago(25 * HOUR), 1),
            EmailHealth::Silent {
                last_email_at: ago(25 * HOUR),
                threads_missed: 1
            }
        );
        assert_eq!(
            health(ago(25 * HOUR), 0),
            EmailHealth::Idle {
                last_email_at: ago(25 * HOUR)
            }
        );
    }

    #[test]
    fn email_health_with_no_email_counts_every_thread() {
        assert_eq!(
            health(None, 0),
            EmailHealth::Idle {
                last_email_at: None
            }
        );
        assert_eq!(
            health(None, 3),
            EmailHealth::Silent {
                last_email_at: None,
                threads_missed: 3
            }
        );
    }

    #[test]
    fn email_health_window_edges() {
        let day = DEFAULT_QUIET_SECS;
        assert!(matches!(
            health(ago(day - 1), 1),
            EmailHealth::Flowing { .. }
        ));
        assert!(matches!(health(ago(day), 1), EmailHealth::Silent { .. }));
        assert!(matches!(health(ago(-60), 1), EmailHealth::Flowing { .. }));
    }

    #[test]
    fn email_health_window_parse_falls_back_to_24_hours() {
        let secs = |raw: Option<&str>| QuietWindow::parse(raw).window.secs();
        for bad in ["abc", "0", "-5", ""] {
            let parsed = QuietWindow::parse(Some(bad));
            assert_eq!(parsed.window.secs(), DEFAULT_QUIET_SECS, "{bad}");
            assert!(parsed.ignored, "{bad}");
        }
        assert_eq!(secs(Some("5")), 5);
        assert_eq!(secs(Some(" 5 ")), 5);
        let missing = QuietWindow::parse(None);
        assert_eq!(missing.window.secs(), DEFAULT_QUIET_SECS);
        assert!(!missing.ignored);
    }

    #[test]
    fn email_health_serializes_with_a_state_tag() {
        let json = |h: EmailHealth| serde_json::to_string(&h).unwrap();
        assert_eq!(
            json(EmailHealth::Idle {
                last_email_at: None
            }),
            r#"{"state":"idle","last_email_at":null}"#
        );
        assert_eq!(
            json(EmailHealth::Silent {
                last_email_at: Some(Timestamp(0)),
                threads_missed: 2
            }),
            r#"{"state":"silent","last_email_at":"1970-01-01T00:00:00Z","threads_missed":2}"#
        );
        let flowing = EmailHealth::Flowing {
            last_email_at: Timestamp(0),
        };
        assert_eq!(
            serde_json::from_str::<EmailHealth>(&json(flowing)).unwrap(),
            flowing
        );
    }
}
