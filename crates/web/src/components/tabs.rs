use domain::api::Counters;

use crate::views;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    YourTurn,
    TheirTurn,
    Pipeline,
    Merged,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::YourTurn, Tab::TheirTurn, Tab::Pipeline, Tab::Merged];

    pub fn label(self) -> &'static str {
        match self {
            Tab::YourTurn => "Your turn",
            Tab::TheirTurn => "Their turn",
            Tab::Pipeline => "Pipeline",
            Tab::Merged => "Merged",
        }
    }

    pub fn href(self) -> &'static str {
        match self {
            Tab::YourTurn => "/",
            Tab::TheirTurn => "/theirs",
            Tab::Pipeline => "/pipeline",
            Tab::Merged => "/merged",
        }
    }

    /// Whether the tab's view exists. Each view module owns its flag.
    pub fn ready(self) -> bool {
        match self {
            Tab::YourTurn => views::your_turn::READY,
            Tab::TheirTurn => views::their_turn::READY,
            Tab::Pipeline => views::pipeline::READY,
            Tab::Merged => views::merged::READY,
        }
    }

    /// The tab's counter (F5).
    pub fn count(self, counters: &Counters) -> u32 {
        match self {
            Tab::YourTurn => counters.your_turn,
            Tab::TheirTurn => counters.their_turn,
            Tab::Pipeline => counters.open_prs,
            Tab::Merged => counters.merged_this_month,
        }
    }

    pub fn from_path(path: &str) -> Option<Tab> {
        let path = if path.len() > 1 {
            path.trim_end_matches('/')
        } else {
            path
        };
        Tab::ALL.into_iter().find(|tab| tab.href() == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_path_round_trips_every_tab() {
        for tab in Tab::ALL {
            assert_eq!(Tab::from_path(tab.href()), Some(tab));
        }
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
        let labels: Vec<_> = Tab::ALL.iter().map(|tab| tab.label()).collect();
        assert_eq!(labels, ["Your turn", "Their turn", "Pipeline", "Merged"]);
    }

    #[test]
    fn each_tab_reads_its_own_counter() {
        let counters = Counters {
            your_turn: 1,
            their_turn: 2,
            open_prs: 3,
            merged_this_month: 4,
        };
        let counts: Vec<_> = Tab::ALL.iter().map(|tab| tab.count(&counters)).collect();
        assert_eq!(counts, [1, 2, 3, 4]);
    }
}
