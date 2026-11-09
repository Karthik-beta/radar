use domain::api::PipelineCard as CardData;
use domain::settings::NudgeThresholds;
use domain::stage::Stage;
use leptos::prelude::*;

use crate::heat::{heat, Heat, Thresholds};
use crate::url::GithubUrl;

const DAY: i64 = 86_400;

/// Days in stage as a card shows it: "today", "1d", "3d".
pub fn days_in_stage(age_secs: i64) -> String {
    match age_secs.max(0) / DAY {
        0 => "today".to_owned(),
        days => format!("{days}d"),
    }
}

/// The heat of a card's age. Only `AwaitingReview` is a wait someone can
/// nudge, so every other stage stays fresh (F4).
pub fn card_heat(stage: Stage, age_secs: i64, nudge: NudgeThresholds) -> Heat {
    if stage != Stage::AwaitingReview {
        return Heat::Fresh;
    }
    heat(
        age_secs,
        Thresholds {
            warm: i64::from(nudge.warm_days()) * DAY,
            hot: i64::from(nudge.hot_days()) * DAY,
        },
    )
}

/// "repo #number", with ", agent" for agent items.
pub fn card_meta(repo: &str, number: u32, agent: bool) -> String {
    if agent {
        format!("{repo} #{number}, agent")
    } else {
        format!("{repo} #{number}")
    }
}

/// One pull request in a stage column: title, then repo and number, then the
/// days in stage.
#[component]
pub fn PipelineCard(
    card: CardData,
    stage: Stage,
    nudge: NudgeThresholds,
    #[prop(into)] now: Signal<i64>,
) -> impl IntoView {
    let since = card.stage_since;
    let age = move || now.get().saturating_sub(since.0);
    let level = move || card_heat(stage, age(), nudge);
    let meta = card_meta(&card.repo.to_string(), card.number, card.agent);
    let title = match GithubUrl::parse(&card.url) {
        Some(url) => view! {
            <a class="card-link" href=url.as_str().to_owned() target="_blank" rel="noopener noreferrer">
                {card.title}
            </a>
        }
        .into_any(),
        None => view! { <span class="card-link">{card.title}</span> }.into_any(),
    };
    view! {
        <li class="card" data-card=card.id.as_str().to_owned()>
            {title}
            <p class="card-meta text-muted">
                <span class="card-repo">{meta}</span>
                <span class=move || format!("age {}", level().class()) data-heat=move || level().as_str()>
                    {move || days_in_stage(age())}
                </span>
            </p>
        </li>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600;

    #[test]
    fn days_in_stage_rounds_down_to_whole_days() {
        assert_eq!(days_in_stage(0), "today");
        assert_eq!(days_in_stage(23 * HOUR), "today");
        assert_eq!(days_in_stage(24 * HOUR), "1d");
        assert_eq!(days_in_stage(3 * DAY + 2 * HOUR), "3d");
        assert_eq!(days_in_stage(-5), "today");
    }

    #[test]
    fn heat_applies_to_awaiting_review_only() {
        let nudge = NudgeThresholds::default();
        let at = |stage, days: i64| card_heat(stage, days * DAY, nudge);
        assert_eq!(at(Stage::AwaitingReview, 6), Heat::Fresh);
        assert_eq!(at(Stage::AwaitingReview, 8), Heat::Warm);
        assert_eq!(at(Stage::AwaitingReview, 15), Heat::Hot);
        for stage in [
            Stage::Draft,
            Stage::CiRunning,
            Stage::CiFailing,
            Stage::ChangesRequested,
            Stage::Approved,
        ] {
            assert_eq!(at(stage, 8), Heat::Fresh, "{stage:?}");
            assert_eq!(at(stage, 15), Heat::Fresh, "{stage:?}");
        }
    }

    #[test]
    fn card_meta_marks_agent_items() {
        assert_eq!(card_meta("octo/widgets", 12, false), "octo/widgets #12");
        assert_eq!(
            card_meta("octo/widgets", 12, true),
            "octo/widgets #12, agent"
        );
    }
}
