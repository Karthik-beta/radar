use domain::stage::Stage;

/// The column heading for a stage. One match, so a new stage fails to compile
/// until it has a label.
pub fn stage_label(stage: Stage) -> &'static str {
    match stage {
        Stage::Draft => "Draft",
        Stage::CiRunning => "CI running",
        Stage::CiFailing => "CI failing",
        Stage::AwaitingReview => "Awaiting review",
        Stage::ChangesRequested => "Changes requested",
        Stage::Approved => "Approved",
        Stage::Merged => "Merged",
        Stage::Closed => "Closed",
    }
}

/// The `data-stage` value, matching the stored snake_case name.
pub fn stage_key(stage: Stage) -> &'static str {
    match stage {
        Stage::Draft => "draft",
        Stage::CiRunning => "ci_running",
        Stage::CiFailing => "ci_failing",
        Stage::AwaitingReview => "awaiting_review",
        Stage::ChangesRequested => "changes_requested",
        Stage::Approved => "approved",
        Stage::Merged => "merged",
        Stage::Closed => "closed",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use domain::api::Pipeline;

    use super::*;

    #[test]
    fn stage_labels_are_distinct_and_not_empty() {
        let labels: HashSet<_> = Stage::ALL.into_iter().map(stage_label).collect();
        assert_eq!(labels.len(), Stage::ALL.len());
        assert!(labels.iter().all(|label| !label.is_empty()));
    }

    #[test]
    fn stage_keys_are_distinct_and_not_empty() {
        let keys: HashSet<_> = Stage::ALL.into_iter().map(stage_key).collect();
        assert_eq!(keys.len(), Stage::ALL.len());
        assert!(keys.iter().all(|key| !key.is_empty()));
    }

    #[test]
    fn stage_board_columns_follow_the_documented_order() {
        let labels: Vec<_> = Pipeline::STAGES.into_iter().map(stage_label).collect();
        assert_eq!(
            labels,
            [
                "Draft",
                "CI running",
                "CI failing",
                "Awaiting review",
                "Changes requested",
                "Approved"
            ]
        );
    }
}
