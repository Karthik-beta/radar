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
            Tab::Pipeline | Tab::Merged => "/pipeline",
        }
    }

    pub fn from_path(path: &str) -> Option<Tab> {
        let path = if path.len() > 1 {
            path.trim_end_matches('/')
        } else {
            path
        };
        Tab::ALL
            .into_iter()
            .find(|tab| *tab != Tab::Merged && tab.href() == path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_path_round_trips_every_routed_tab() {
        for tab in Tab::ALL.into_iter().filter(|tab| *tab != Tab::Merged) {
            assert_eq!(Tab::from_path(tab.href()), Some(tab));
        }
    }

    #[test]
    fn merged_links_to_the_pipeline() {
        assert_eq!(Tab::Merged.href(), Tab::Pipeline.href());
        assert_eq!(Tab::from_path(Tab::Merged.href()), Some(Tab::Pipeline));
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
}
