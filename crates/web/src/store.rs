use std::collections::{HashMap, HashSet};
use std::time::Duration;

use domain::api::{Dashboard, Freshness, ItemId, SnoozeFor};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, ApiError};

const REFETCH_EVERY: Duration = Duration::from_secs(60);
const CLOCK_EVERY: Duration = Duration::from_secs(15);

/// What the app holds from `GET /api/dashboard`. `fetched_at` is the
/// browser's clock in milliseconds at the last successful fetch.
#[derive(Debug, Clone, PartialEq)]
pub enum Load {
    Loading,
    Ready {
        data: Dashboard,
        fetched_at: f64,
    },
    Stale {
        data: Dashboard,
        fetched_at: f64,
        error: ApiError,
    },
    SignedOut,
    Failed(ApiError),
}

impl Load {
    pub fn data(&self) -> Option<&Dashboard> {
        match self {
            Self::Ready { data, .. } | Self::Stale { data, .. } => Some(data),
            _ => None,
        }
    }

    /// The state after a fetch ends. A failure keeps rows already on screen.
    pub fn settle(self, result: Result<Dashboard, ApiError>, now: f64) -> Self {
        match (self, result) {
            (_, Ok(data)) => Self::Ready {
                data,
                fetched_at: now,
            },
            (_, Err(ApiError::Unauthorized)) => Self::SignedOut,
            (
                Self::Ready { data, fetched_at }
                | Self::Stale {
                    data, fetched_at, ..
                },
                Err(error),
            ) => Self::Stale {
                data,
                fetched_at,
                error,
            },
            (_, Err(error)) => Self::Failed(error),
        }
    }
}

/// What the app holds from `GET /api/freshness`. A failed fetch is
/// `Unavailable` and keeps the last answer for the panel.
#[derive(Debug, Clone, PartialEq)]
pub enum FreshLoad {
    Loading,
    Ready(Freshness),
    Unavailable(Option<Freshness>),
}

impl FreshLoad {
    /// The last answer, even when the latest fetch failed.
    pub fn data(&self) -> Option<&Freshness> {
        match self {
            Self::Ready(data) => Some(data),
            Self::Unavailable(last) => last.as_ref(),
            Self::Loading => None,
        }
    }

    pub fn settle(self, result: Result<Freshness, ApiError>) -> Self {
        match result {
            Ok(data) => Self::Ready(data),
            Err(_) => Self::Unavailable(match self {
                Self::Ready(data) => Some(data),
                Self::Unavailable(last) => last,
                Self::Loading => None,
            }),
        }
    }
}

/// A triage action the operator took on one item (F9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Triage {
    Dismiss,
    Snooze(SnoozeFor),
}

impl Triage {
    pub fn failure(self) -> &'static str {
        match self {
            Self::Dismiss => "Could not dismiss. Try again.",
            Self::Snooze(_) => "Could not snooze. Try again.",
        }
    }
}

/// An item hidden before the Worker confirmed it. `Sent` waits for a fetch
/// that started after the POST succeeded, because an earlier one may still
/// list the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    InFlight,
    Sent { after_fetch: u64 },
}

/// The dashboard with pending items taken out of the lists and the counters.
/// Rows still sliding out stay in the lists so they can animate.
pub fn visible(
    data: &Dashboard,
    pending: &HashMap<ItemId, Pending>,
    leaving: &HashSet<ItemId>,
) -> Dashboard {
    let mut out = data.clone();
    let shown = |id: &ItemId| !pending.contains_key(id) || leaving.contains(id);
    let held = |ids: &mut dyn Iterator<Item = &ItemId>| {
        u32::try_from(ids.filter(|id| pending.contains_key(*id)).count()).unwrap_or(u32::MAX)
    };
    out.counters.your_turn = out
        .counters
        .your_turn
        .saturating_sub(held(&mut data.your_turn.iter().map(|r| &r.id)));
    out.counters.their_turn = out
        .counters
        .their_turn
        .saturating_sub(held(&mut data.their_turn.iter().map(|r| &r.id)));
    out.your_turn.retain(|row| shown(&row.id));
    out.their_turn.retain(|row| shown(&row.id));
    out
}

/// The app's one copy of the dashboard, shared through context. Every view
/// reads `dashboard()`, which already hides items being dismissed or snoozed.
#[derive(Clone, Copy)]
pub struct Store {
    load: RwSignal<Load>,
    now: RwSignal<f64>,
    notice: RwSignal<Option<&'static str>>,
    pending: RwSignal<HashMap<ItemId, Pending>>,
    leaving: RwSignal<HashSet<ItemId>>,
    in_flight: StoredValue<bool>,
    fetches: StoredValue<u64>,
    /// Bumped on sign-out, so a fetch that started earlier is ignored.
    session: StoredValue<u64>,
    view: Memo<Option<Dashboard>>,
    fresh: RwSignal<FreshLoad>,
    fresh_in_flight: StoredValue<bool>,
}

pub fn provide_store() -> Store {
    let load = RwSignal::new(Load::Loading);
    let pending = RwSignal::new(HashMap::new());
    let leaving = RwSignal::new(HashSet::new());
    let view = Memo::new(move |_| {
        load.with(|load| {
            load.data()
                .map(|data| pending.with(|p| leaving.with(|l| visible(data, p, l))))
        })
    });
    let store = Store {
        load,
        now: RwSignal::new(now_ms()),
        notice: RwSignal::new(None),
        pending,
        leaving,
        in_flight: StoredValue::new(false),
        fetches: StoredValue::new(0),
        session: StoredValue::new(0),
        view,
        fresh: RwSignal::new(FreshLoad::Loading),
        fresh_in_flight: StoredValue::new(false),
    };
    provide_context(store);
    store
}

pub fn use_store() -> Store {
    expect_context::<Store>()
}

impl Store {
    pub fn load(&self) -> Signal<Load> {
        self.load.into()
    }

    pub fn dashboard(&self) -> Memo<Option<Dashboard>> {
        self.view
    }

    /// The browser clock in seconds, ticking every 15 s, for ages.
    pub fn now_secs(&self) -> Signal<i64> {
        let now = self.now;
        Signal::derive(move || (now.get() / 1000.0) as i64)
    }

    /// The last triage failure, cleared by the next triage.
    pub fn notice(&self) -> Signal<Option<&'static str>> {
        self.notice.into()
    }

    /// Whether the row is sliding out. Reactive.
    pub fn is_leaving(&self, id: &ItemId) -> bool {
        self.leaving.with(|l| l.contains(id))
    }

    /// The last `GET /api/freshness` answer, for the header and the banner.
    pub fn freshness(&self) -> Signal<FreshLoad> {
        self.fresh.into()
    }

    fn refresh_freshness(&self) {
        if self.fresh_in_flight.get_value() {
            return;
        }
        self.fresh_in_flight.set_value(true);
        let store = *self;
        spawn_local(async move {
            let result = api::freshness().await;
            store.now.set(now_ms());
            store.fresh.update(|fresh| {
                *fresh = std::mem::replace(fresh, FreshLoad::Loading).settle(result);
            });
            store.fresh_in_flight.set_value(false);
        });
    }

    /// Fetches the dashboard and freshness, each unless one is already running.
    pub fn refresh(&self) {
        self.refresh_freshness();
        if self.in_flight.get_value() {
            return;
        }
        self.in_flight.set_value(true);
        let fetch = self.fetches.get_value() + 1;
        self.fetches.set_value(fetch);
        let session = self.session.get_value();
        let store = *self;
        spawn_local(async move {
            let result = api::dashboard().await;
            if store.session.get_value() != session {
                store.in_flight.set_value(false);
                return;
            }
            let now = now_ms();
            if result.is_ok() {
                store.pending.update(|p| {
                    p.retain(|_, state| match state {
                        Pending::InFlight => true,
                        Pending::Sent { after_fetch } => fetch < *after_fetch,
                    })
                });
            }
            store.load.update(|load| {
                *load = std::mem::replace(load, Load::Loading).settle(result, now);
            });
            store.now.set(now);
            store.in_flight.set_value(false);
        });
    }

    /// Hides the item at once, slides it out, and posts the action. A failed
    /// post brings the row back with a message.
    pub fn triage(&self, id: ItemId, action: Triage) {
        let store = *self;
        self.notice.set(None);
        self.pending.update(|p| {
            p.insert(id.clone(), Pending::InFlight);
        });
        self.leaving.update(|l| {
            l.insert(id.clone());
        });
        let slid = id.clone();
        set_timeout(
            move || {
                store.leaving.update(|l| {
                    l.remove(&slid);
                })
            },
            dismiss_duration(),
        );
        spawn_local(async move {
            let result = match action {
                Triage::Dismiss => api::dismiss(&id).await,
                Triage::Snooze(snooze_for) => api::snooze(&id, snooze_for).await,
            };
            match result {
                Ok(()) => {
                    let after_fetch = store.fetches.get_value() + 1;
                    store.pending.update(|p| {
                        p.insert(id, Pending::Sent { after_fetch });
                    });
                }
                Err(ApiError::Unauthorized) => {
                    store.session.update_value(|s| *s += 1);
                    store.load.set(Load::SignedOut);
                }
                Err(_) => {
                    store.pending.update(|p| {
                        p.remove(&id);
                    });
                    store.leaving.update(|l| {
                        l.remove(&id);
                    });
                    store.notice.set(Some(action.failure()));
                }
            }
        });
    }

    /// Loads now, then refetches on focus, when the tab shows again, and every
    /// 60 s while it is visible. Call once, from the signed-in shell.
    pub fn start(&self) {
        let store = *self;
        store.refresh();
        let on_focus = window_event_listener(leptos::ev::focus, move |_| store.refresh());
        let on_show = leptos::leptos_dom::helpers::window_event_listener_untyped(
            "visibilitychange",
            move |_| {
                if !hidden() {
                    store.refresh();
                }
            },
        );
        let poll = set_interval_with_handle(
            move || {
                if !hidden() {
                    store.refresh();
                }
            },
            REFETCH_EVERY,
        )
        .ok();
        let clock = set_interval_with_handle(move || store.now.set(now_ms()), CLOCK_EVERY).ok();
        on_cleanup(move || {
            on_focus.remove();
            on_show.remove();
            poll.into_iter()
                .chain(clock)
                .for_each(|handle| handle.clear());
        });
    }
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

fn hidden() -> bool {
    document().hidden()
}

/// `motion.duration.dismiss`, read from the generated custom property so the
/// timer and the CSS transition agree.
fn dismiss_duration() -> Duration {
    let ms = document()
        .document_element()
        .and_then(|root| window().get_computed_style(&root).ok().flatten())
        .and_then(|style| style.get_property_value("--duration-dismiss").ok())
        .and_then(|value| value.trim().strip_suffix("ms")?.trim().parse::<u64>().ok())
        .unwrap_or(0);
    Duration::from_millis(ms)
}

#[cfg(test)]
mod tests {
    use domain::api::{Counters, Pipeline, TurnRow};
    use domain::ids::{ItemKind, RepoName, Timestamp};
    use domain::settings::NudgeThresholds;
    use domain::turn::Reason;

    use super::*;

    fn row(id: &str) -> TurnRow {
        TurnRow {
            id: ItemId::parse(id).expect("id"),
            kind: ItemKind::Pr,
            repo: RepoName::parse("o/r").expect("repo"),
            number: 1,
            title: "t".into(),
            url: "https://github.com/o/r/pull/1".into(),
            reason: Reason::Mentioned,
            actor: None,
            what: None,
            since: Timestamp(0),
            agent: false,
        }
    }

    fn dashboard(ids: &[&str]) -> Dashboard {
        Dashboard {
            generated_at: Timestamp(0),
            counters: Counters {
                your_turn: u32::try_from(ids.len()).expect("small"),
                their_turn: 0,
                open_prs: 0,
                merged_this_month: 0,
            },
            your_turn: ids.iter().map(|id| row(id)).collect(),
            their_turn: Vec::new(),
            pipeline: Pipeline::empty(),
            recent: Vec::new(),
            nudge: NudgeThresholds::new(7, 14).expect("nudge"),
        }
    }

    #[test]
    fn store_a_failed_refetch_keeps_the_rows() {
        let ready = Load::Loading.settle(Ok(dashboard(&["A"])), 5.0);
        let stale = ready.clone().settle(Err(ApiError::Status(500)), 9.0);
        assert_eq!(stale.data(), ready.data());
        assert!(matches!(
            stale,
            Load::Stale {
                error: ApiError::Status(500),
                ..
            }
        ));
        assert!(matches!(
            stale.settle(Ok(dashboard(&[])), 12.0),
            Load::Ready { fetched_at, .. } if fetched_at == 12.0
        ));
    }

    fn freshness() -> Freshness {
        Freshness {
            sources: Vec::new(),
            budget: domain::api::Budget {
                limit: None,
                remaining: None,
                reset_at: None,
            },
            dead: Vec::new(),
            scope: domain::api::ScopeState::Ok,
            next_poll_at: None,
            email: domain::email_health::EmailHealth::Idle {
                last_email_at: None,
            },
        }
    }

    #[test]
    fn store_a_failed_freshness_fetch_keeps_the_last_answer() {
        let ready = FreshLoad::Loading.settle(Ok(freshness()));
        let down = ready.settle(Err(ApiError::Status(500)));
        assert!(matches!(down, FreshLoad::Unavailable(Some(_))));
        assert_eq!(down.data(), Some(&freshness()));
        assert_eq!(
            FreshLoad::Loading.settle(Err(ApiError::Network)),
            FreshLoad::Unavailable(None)
        );
        assert!(matches!(down.settle(Ok(freshness())), FreshLoad::Ready(_)));
    }

    #[test]
    fn store_a_401_signs_out_and_a_first_failure_fails() {
        let ready = Load::Loading.settle(Ok(dashboard(&["A"])), 5.0);
        assert_eq!(
            ready.settle(Err(ApiError::Unauthorized), 6.0),
            Load::SignedOut
        );
        assert_eq!(
            Load::Loading.settle(Err(ApiError::Network), 6.0),
            Load::Failed(ApiError::Network)
        );
    }

    #[test]
    fn store_pending_items_leave_the_list_and_the_counter() {
        let data = dashboard(&["A", "B", "C"]);
        let pending = HashMap::from([
            (ItemId::parse("A").expect("id"), Pending::InFlight),
            (
                ItemId::parse("B").expect("id"),
                Pending::Sent { after_fetch: 2 },
            ),
        ]);
        let gone = visible(&data, &pending, &HashSet::new());
        assert_eq!(gone.counters.your_turn, 1);
        assert_eq!(gone.your_turn.len(), 1);
        let sliding = visible(
            &data,
            &pending,
            &HashSet::from([ItemId::parse("A").expect("id")]),
        );
        assert_eq!(sliding.counters.your_turn, 1);
        assert_eq!(sliding.your_turn.len(), 2);
    }
}
