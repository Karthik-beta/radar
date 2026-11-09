use domain::api::{ItemId, SnoozeFor};
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, KeyboardEvent};

use crate::components::triage_row::RowData;
use crate::url::GithubUrl;

/// A row the keyboard can act on.
pub trait Triageable {
    fn item_id(&self) -> &ItemId;
    fn item_url(&self) -> &str;
}

impl Triageable for RowData {
    fn item_id(&self) -> &ItemId {
        &self.id
    }
    fn item_url(&self) -> &str {
        self.url.as_ref().map_or("", GithubUrl::as_str)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Down,
    Up,
    Open,
    Dismiss,
    Snooze,
    Day,
    Week,
    Escape,
}

impl Key {
    pub fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "j" => Self::Down,
            "k" => Self::Up,
            "Enter" => Self::Open,
            "e" => Self::Dismiss,
            "s" => Self::Snooze,
            "d" => Self::Day,
            "w" => Self::Week,
            "Escape" => Self::Escape,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Browsing,
    Snoozing { item: ItemId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    None,
    At(usize),
}

impl Selection {
    /// The selection a list of `len` rows can hold: the first row when nothing
    /// is selected yet, the last row when the list shrank under it.
    pub fn clamp(self, len: usize) -> Self {
        match (self, len) {
            (_, 0) => Self::None,
            (Self::None, _) => Self::At(0),
            (Self::At(i), len) => Self::At(i.min(len - 1)),
        }
    }

    pub fn index(self) -> Option<usize> {
        match self {
            Self::None => None,
            Self::At(i) => Some(i),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub mode: Mode,
    pub selection: Selection,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::Browsing,
            selection: Selection::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    Open(GithubUrl),
    Dismiss(ItemId),
    Snooze(ItemId, SnoozeFor),
}

/// The pure key reducer. `rows` are the rows on screen, in order. Dismissing
/// or snoozing keeps the index, clamped to the list one row shorter.
pub fn step<R: Triageable>(state: State, key: Key, rows: &[R]) -> (State, Option<Effect>) {
    let len = rows.len();
    let selection = state.selection.clamp(len);
    let selected = selection.index().and_then(|i| rows.get(i));
    match state.mode {
        Mode::Snoozing { item } => {
            let snooze = |snooze_for| {
                let after = State {
                    mode: Mode::Browsing,
                    selection: selection.clamp(len.saturating_sub(1)),
                };
                (after, Some(Effect::Snooze(item.clone(), snooze_for)))
            };
            match key {
                Key::Day => snooze(SnoozeFor::Day),
                Key::Week => snooze(SnoozeFor::Week),
                Key::Escape => (
                    State {
                        mode: Mode::Browsing,
                        selection,
                    },
                    None,
                ),
                _ => (
                    State {
                        mode: Mode::Snoozing { item },
                        selection,
                    },
                    None,
                ),
            }
        }
        Mode::Browsing => {
            let browsing = |selection| State {
                mode: Mode::Browsing,
                selection,
            };
            match (key, selection, selected) {
                (Key::Down, Selection::At(i), _) => {
                    (browsing(Selection::At(i + 1).clamp(len)), None)
                }
                (Key::Up, Selection::At(i), _) => {
                    (browsing(Selection::At(i.saturating_sub(1))), None)
                }
                (Key::Open, _, Some(row)) => (
                    browsing(selection),
                    GithubUrl::parse(row.item_url()).map(Effect::Open),
                ),
                (Key::Dismiss, _, Some(row)) => (
                    browsing(selection.clamp(len - 1)),
                    Some(Effect::Dismiss(row.item_id().clone())),
                ),
                (Key::Snooze, _, Some(row)) => (
                    State {
                        mode: Mode::Snoozing {
                            item: row.item_id().clone(),
                        },
                        selection,
                    },
                    None,
                ),
                _ => (browsing(selection), None),
            }
        }
    }
}

/// Reads a keydown into a `Key`, ignoring keys with a modifier and keys typed
/// into a field.
pub fn from_event(event: &KeyboardEvent) -> Option<Key> {
    if event.ctrl_key() || event.meta_key() || event.alt_key() || event.shift_key() {
        return None;
    }
    let typing = event
        .target()
        .and_then(|target| target.dyn_into::<HtmlElement>().ok())
        .is_some_and(|el| {
            el.is_content_editable()
                || matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT")
        });
    if typing {
        return None;
    }
    Key::parse(&event.key())
}

/// Calls `on_key` for each triage key until the calling component unmounts.
pub fn listen(on_key: impl Fn(Key) + 'static) {
    let handle = window_event_listener(leptos::ev::keydown, move |event| {
        if let Some(key) = from_event(&event) {
            event.prevent_default();
            on_key(key);
        }
    });
    on_cleanup(move || handle.remove());
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Row(ItemId, &'static str);

    impl Triageable for Row {
        fn item_id(&self) -> &ItemId {
            &self.0
        }
        fn item_url(&self) -> &str {
            self.1
        }
    }

    fn rows(n: usize) -> Vec<Row> {
        (0..n)
            .map(|i| {
                Row(
                    ItemId::parse(&format!("I_{i}")).expect("id"),
                    "https://github.com/o/r/pull/1",
                )
            })
            .collect()
    }

    fn at(i: usize) -> State {
        State {
            mode: Mode::Browsing,
            selection: Selection::At(i),
        }
    }

    fn id(i: usize) -> ItemId {
        ItemId::parse(&format!("I_{i}")).expect("id")
    }

    #[test]
    fn keys_clamp_at_both_ends() {
        let list = rows(3);
        assert_eq!(step(at(2), Key::Down, &list), (at(2), None));
        assert_eq!(step(at(0), Key::Up, &list), (at(0), None));
        assert_eq!(step(at(0), Key::Down, &list), (at(1), None));
    }

    #[test]
    fn keys_start_on_the_first_row() {
        let list = rows(3);
        assert_eq!(step(State::default(), Key::Down, &list).0, at(1));
        assert_eq!(State::default().selection.clamp(3), Selection::At(0));
    }

    #[test]
    fn keys_never_select_in_an_empty_list() {
        let empty: Vec<Row> = Vec::new();
        for key in [
            Key::Down,
            Key::Up,
            Key::Open,
            Key::Dismiss,
            Key::Snooze,
            Key::Escape,
        ] {
            let (state, effect) = step(State::default(), key, &empty);
            assert_eq!(state.selection, Selection::None, "{key:?}");
            assert_eq!(state.mode, Mode::Browsing, "{key:?}");
            assert_eq!(effect, None, "{key:?}");
        }
    }

    #[test]
    fn keys_dismissing_the_last_row_selects_the_new_last_row() {
        let list = rows(3);
        assert_eq!(
            step(at(2), Key::Dismiss, &list),
            (at(1), Some(Effect::Dismiss(id(2))))
        );
        assert_eq!(
            step(at(0), Key::Dismiss, &list),
            (at(0), Some(Effect::Dismiss(id(0))))
        );
        let (state, _) = step(at(0), Key::Dismiss, &rows(1));
        assert_eq!(state.selection, Selection::None);
    }

    #[test]
    fn keys_snooze_then_week() {
        let list = rows(3);
        let (snoozing, effect) = step(at(1), Key::Snooze, &list);
        assert_eq!(snoozing.mode, Mode::Snoozing { item: id(1) });
        assert_eq!(effect, None);
        assert_eq!(
            step(snoozing, Key::Week, &list),
            (at(1), Some(Effect::Snooze(id(1), SnoozeFor::Week)))
        );
    }

    #[test]
    fn keys_escape_cancels_snoozing() {
        let list = rows(3);
        let (snoozing, _) = step(at(1), Key::Snooze, &list);
        assert_eq!(step(snoozing, Key::Escape, &list), (at(1), None));
    }

    #[test]
    fn keys_do_nothing_else_while_snoozing() {
        let list = rows(3);
        let (snoozing, _) = step(at(1), Key::Snooze, &list);
        for key in [Key::Dismiss, Key::Down, Key::Up, Key::Open, Key::Snooze] {
            assert_eq!(
                step(snoozing.clone(), key, &list),
                (snoozing.clone(), None),
                "{key:?}"
            );
        }
    }

    #[test]
    fn keys_open_only_github_urls() {
        let good = rows(1);
        assert_eq!(
            step(at(0), Key::Open, &good).1,
            GithubUrl::parse("https://github.com/o/r/pull/1").map(Effect::Open)
        );
        let bad = vec![Row(id(0), "javascript:alert(1)")];
        assert_eq!(step(at(0), Key::Open, &bad).1, None);
    }

    #[test]
    fn keys_parse_names_the_triage_keys_only() {
        assert_eq!(Key::parse("j"), Some(Key::Down));
        assert_eq!(Key::parse("Enter"), Some(Key::Open));
        assert_eq!(Key::parse("J"), None);
        assert_eq!(Key::parse("x"), None);
    }
}
