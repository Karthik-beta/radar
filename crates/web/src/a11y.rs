use domain::api::SnoozeFor;

use crate::components::tabs::Tab;

/// The text a screen reader hears after a triage action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Announce {
    Dismissed,
    Snoozed(SnoozeFor),
}

/// "Your turn (3) · Radar", or "Your turn · Radar" until the counters load.
pub fn document_title(tab: Tab, count: Option<u32>) -> String {
    match count {
        Some(n) => format!("{} ({n}) · Radar", tab.label()),
        None => format!("{} · Radar", tab.label()),
    }
}

/// "Dismissed. 2 left.", and "Nothing left." at zero.
pub fn live_text(action: Announce, left: usize) -> String {
    let what = match action {
        Announce::Dismissed => "Dismissed.",
        Announce::Snoozed(SnoozeFor::Day) => "Snoozed for 1 day.",
        Announce::Snoozed(SnoozeFor::Week) => "Snoozed for 1 week.",
    };
    match left {
        0 => format!("{what} Nothing left."),
        n => format!("{what} {n} left."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_title_names_the_tab_and_its_count() {
        assert_eq!(
            document_title(Tab::YourTurn, Some(3)),
            "Your turn (3) · Radar"
        );
        assert_eq!(
            document_title(Tab::Pipeline, Some(0)),
            "Pipeline (0) · Radar"
        );
        assert_eq!(document_title(Tab::TheirTurn, None), "Their turn · Radar");
    }

    #[test]
    fn live_text_reads_dismiss_and_snooze_with_the_rows_left() {
        assert_eq!(live_text(Announce::Dismissed, 4), "Dismissed. 4 left.");
        assert_eq!(live_text(Announce::Dismissed, 1), "Dismissed. 1 left.");
        assert_eq!(
            live_text(Announce::Dismissed, 0),
            "Dismissed. Nothing left."
        );
        assert_eq!(
            live_text(Announce::Snoozed(SnoozeFor::Week), 2),
            "Snoozed for 1 week. 2 left."
        );
        assert_eq!(
            live_text(Announce::Snoozed(SnoozeFor::Day), 0),
            "Snoozed for 1 day. Nothing left."
        );
    }
}
