use domain::api::Counters;

use crate::views;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    YourTurn,
    TheirTurn,
    Pipeline,
    Merged,
    Feed,
    Trends,
}

/// `Counters` are the four tabs of ADR 0003, each with a count. `More` is the
/// second, quieter group at the end of the header, with no counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavGroup {
    Counters,
    More,
}

/// Every tab, in header order, with its label, path, and group. A new tab is a
/// variant, one row here, and a `ready` arm.
const TABS: [(Tab, &str, &str, NavGroup); 6] = [
    (Tab::YourTurn, "Your turn", "/", NavGroup::Counters),
    (Tab::TheirTurn, "Their turn", "/theirs", NavGroup::Counters),
    (Tab::Pipeline, "Pipeline", "/pipeline", NavGroup::Counters),
    (Tab::Merged, "Merged", "/merged", NavGroup::Counters),
    (Tab::Feed, "Feed", "/feed", NavGroup::More),
    (Tab::Trends, "Trends", "/trends", NavGroup::More),
];

impl Tab {
    fn row(self) -> (Tab, &'static str, &'static str, NavGroup) {
        TABS.into_iter()
            .find(|(tab, ..)| *tab == self)
            .unwrap_or(TABS[0])
    }

    pub fn in_group(group: NavGroup) -> impl Iterator<Item = Tab> {
        TABS.into_iter()
            .map(|(tab, ..)| tab)
            .filter(move |tab| tab.group() == group)
    }

    pub fn label(self) -> &'static str {
        self.row().1
    }

    pub fn href(self) -> &'static str {
        self.row().2
    }

    pub fn group(self) -> NavGroup {
        self.row().3
    }

    /// Whether the tab's view exists. Each view module owns its flag.
    pub fn ready(self) -> bool {
        match self {
            Tab::YourTurn => views::your_turn::READY,
            Tab::TheirTurn => views::their_turn::READY,
            Tab::Pipeline => views::pipeline::READY,
            Tab::Merged => views::merged::READY,
            Tab::Feed => views::feed::READY,
            Tab::Trends => views::trends::READY,
        }
    }

    /// The tab's counter (F5). Tabs outside the counter group have none.
    pub fn count(self, counters: &Counters) -> Option<u32> {
        match self {
            Tab::YourTurn => Some(counters.your_turn),
            Tab::TheirTurn => Some(counters.their_turn),
            Tab::Pipeline => Some(counters.open_prs),
            Tab::Merged => Some(counters.merged_this_month),
            Tab::Feed | Tab::Trends => None,
        }
    }

    pub fn from_path(path: &str) -> Option<Tab> {
        let path = if path.len() > 1 {
            path.trim_end_matches('/')
        } else {
            path
        };
        TABS.into_iter()
            .find(|(_, _, href, _)| *href == path)
            .map(|(tab, ..)| tab)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_path_round_trips_every_tab() {
        for (tab, ..) in TABS {
            assert_eq!(Tab::from_path(tab.href()), Some(tab));
        }
    }

    #[test]
    fn every_tab_has_one_row_and_a_group() {
        for (tab, label, href, group) in TABS {
            assert_eq!((tab.label(), tab.href(), tab.group()), (label, href, group));
        }
        assert_eq!(Tab::from_path("/feed"), Some(Tab::Feed));
        assert_eq!(Tab::Feed.group(), NavGroup::More);
        assert_eq!(Tab::YourTurn.group(), NavGroup::Counters);
    }

    #[test]
    fn unknown_path_is_none() {
        assert_eq!(Tab::from_path("/zzz"), None);
        assert_eq!(Tab::from_path("/pipeline/extra"), None);
        assert_eq!(Tab::from_path(""), None);
    }

    #[test]
    fn trailing_slash_is_ignored() {
        assert_eq!(Tab::from_path("/pipeline/"), Some(Tab::Pipeline));
        assert_eq!(Tab::from_path("/"), Some(Tab::YourTurn));
    }

    #[test]
    fn all_follows_the_adr_order() {
        let labels: Vec<_> = Tab::in_group(NavGroup::Counters)
            .map(|tab| tab.label())
            .collect();
        assert_eq!(labels, ["Your turn", "Their turn", "Pipeline", "Merged"]);
        let more: Vec<_> = Tab::in_group(NavGroup::More).collect();
        assert_eq!(more, [Tab::Feed, Tab::Trends]);
    }

    #[test]
    fn each_tab_reads_its_own_counter() {
        let counters = Counters {
            your_turn: 1,
            their_turn: 2,
            open_prs: 3,
            merged_this_month: 4,
        };
        let counts: Vec<_> = Tab::in_group(NavGroup::Counters)
            .map(|tab| tab.count(&counters))
            .collect();
        assert_eq!(counts, [Some(1), Some(2), Some(3), Some(4)]);
        assert_eq!(Tab::Feed.count(&counters), None);
        assert_eq!(Tab::Trends.count(&counters), None);
    }
}
