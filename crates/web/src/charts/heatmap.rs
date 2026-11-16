use domain::trends::{DayCell, HeatLevel, HEATMAP_WEEKS};
use leptos::prelude::*;

use super::table::data_table;
use super::{events_title, iso_date};
use crate::tokens;

const CELL: usize = tokens::CELL as usize;
const GAP: usize = tokens::GAP as usize;
const DAYS: usize = 7;
const WIDTH: usize = HEATMAP_WEEKS * CELL + (HEATMAP_WEEKS - 1) * GAP;
const HEIGHT: usize = DAYS * CELL + (DAYS - 1) * GAP;

fn level_class(level: HeatLevel) -> &'static str {
    match level {
        HeatLevel::None => "heat-none",
        HeatLevel::Low => "heat-low",
        HeatLevel::Mid => "heat-mid",
        HeatLevel::High => "heat-high",
    }
}

/// The top-left corner of the cell at `index`, which runs by week and then by
/// weekday from Monday.
fn origin(index: usize) -> (usize, usize) {
    let (column, row) = (index / DAYS, index % DAYS);
    (column * (CELL + GAP), row * (CELL + GAP))
}

/// Events by me per day over 26 weeks, one column per week. The cells are not
/// focusable. The hidden table holds each week's total.
pub fn heat_map(cells: Vec<DayCell>) -> AnyView {
    let total: u32 = cells.iter().map(|c| c.count).sum();
    let label = format!("Daily activity over {HEATMAP_WEEKS} weeks, {total} events");
    let weeks: Vec<(String, u32)> = cells
        .chunks(DAYS)
        .map(|week| {
            (
                week.first().map(|c| iso_date(c.date)).unwrap_or_default(),
                week.iter().map(|c| c.count).sum(),
            )
        })
        .collect();
    view! {
        <figure class="chart">
            <svg
                class="heat-svg"
                width=WIDTH.to_string()
                height=HEIGHT.to_string()
                viewBox=format!("0 0 {WIDTH} {HEIGHT}")
                role="img"
                aria-label=label
            >
                {cells
                    .iter()
                    .enumerate()
                    .map(|(index, cell)| {
                        let (x, y) = origin(index);
                        view! {
                            <rect
                                class=level_class(cell.level)
                                x=x.to_string()
                                y=y.to_string()
                                width=CELL.to_string()
                                height=CELL.to_string()
                                rx=tokens::CELL_RADIUS.to_string()
                            >
                                <title>{events_title(cell.count, cell.date)}</title>
                            </rect>
                        }
                    })
                    .collect_view()}
            </svg>
            {data_table(
                "Events per week".to_owned(),
                vec!["Week of".to_owned(), "Events".to_owned()],
                weeks.into_iter().map(|(week, count)| vec![week, count.to_string()]).collect(),
            )}
        </figure>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_is_335_by_88_and_fits_a_phone() {
        assert_eq!((WIDTH, HEIGHT), (335, 88));
        const { assert!(WIDTH + 2 * 16 <= 375) };
    }

    #[test]
    fn cells_run_down_a_week_then_across() {
        assert_eq!(origin(0), (0, 0));
        assert_eq!(origin(6), (0, 6 * 13));
        assert_eq!(origin(7), (13, 0));
        assert_eq!(origin(181), (25 * 13, 6 * 13));
    }
}
