use domain::trends::HeatLevel;
use leptos::prelude::*;

use super::table::data_table;

const CELL: usize = 10;
const GAP: usize = 2;
const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const HOURS: usize = 24;
const LEFT: usize = 30;
const TOP: usize = 14;
const WIDTH: usize = LEFT + HOURS * (CELL + GAP);
const HEIGHT: usize = TOP + 7 * (CELL + GAP);
/// The hours that carry a label.
const LABELLED: [usize; 4] = [0, 6, 12, 18];

fn level(count: u32, max: u32) -> HeatLevel {
    if count == 0 || max == 0 {
        return HeatLevel::None;
    }
    match u64::from(count) * 3 {
        n if n <= u64::from(max) => HeatLevel::Low,
        n if n <= u64::from(max) * 2 => HeatLevel::Mid,
        _ => HeatLevel::High,
    }
}

fn level_class(level: HeatLevel) -> &'static str {
    match level {
        HeatLevel::None => "heat-none",
        HeatLevel::Low => "heat-low",
        HeatLevel::Mid => "heat-mid",
        HeatLevel::High => "heat-high",
    }
}

/// The cell's top-left corner, from its weekday row and hour column.
fn origin(day: usize, hour: usize) -> (usize, usize) {
    (LEFT + hour * (CELL + GAP), TOP + day * (CELL + GAP))
}

/// Events by weekday and hour, Monday first. The cells are not focusable, and
/// the hidden table holds every count.
pub fn habit_grid(grid: Vec<Vec<u32>>, label: String) -> AnyView {
    let max = grid.iter().flatten().copied().max().unwrap_or(0);
    let cells = grid
        .iter()
        .zip(DAYS)
        .enumerate()
        .flat_map(|(day, (row, _))| {
            row.iter()
                .take(HOURS)
                .enumerate()
                .map(move |(hour, count)| (day, hour, *count))
        })
        .map(|(day, hour, count)| {
            let (x, y) = origin(day, hour);
            view! {
                <rect class=level_class(level(count, max)) x=x.to_string() y=y.to_string() width=CELL.to_string() height=CELL.to_string() rx="2" />
            }
        })
        .collect_view();
    let days = DAYS
        .into_iter()
        .enumerate()
        .map(|(day, name)| {
            view! {
                <text class="chart-label" x="0" y=(origin(day, 0).1 + CELL - 1).to_string()>
                    {name}
                </text>
            }
        })
        .collect_view();
    let hours = LABELLED
        .into_iter()
        .map(|hour| {
            view! {
                <text class="chart-label" x=origin(0, hour).0.to_string() y=(TOP - 4).to_string()>
                    {format!("{hour:02}")}
                </text>
            }
        })
        .collect_view();
    let head = std::iter::once("Day".to_owned())
        .chain((0..HOURS).map(|hour| format!("{hour:02}")))
        .collect();
    let rows = grid
        .iter()
        .zip(DAYS)
        .map(|(row, name)| {
            std::iter::once(name.to_owned())
                .chain(row.iter().take(HOURS).map(|count| count.to_string()))
                .collect()
        })
        .collect();
    view! {
        <figure class="chart">
            <svg class="grid-svg" viewBox=format!("0 0 {WIDTH} {HEIGHT}") role="img" aria-label=label.clone()>
                {days}
                {hours}
                {cells}
            </svg>
            {data_table(label, head, rows)}
        </figure>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_fits_a_phone() {
        assert_eq!((WIDTH, HEIGHT), (318, 98));
        const { assert!(WIDTH + 2 * 16 <= 375) };
    }

    #[test]
    fn levels_follow_the_busiest_cell() {
        assert_eq!(level(0, 9), HeatLevel::None);
        assert_eq!(level(3, 9), HeatLevel::Low);
        assert_eq!(level(6, 9), HeatLevel::Mid);
        assert_eq!(level(9, 9), HeatLevel::High);
        assert_eq!(level(1, 0), HeatLevel::None);
    }

    #[test]
    fn cells_run_across_an_hour_row_and_down_the_days() {
        assert_eq!(origin(0, 0), (LEFT, TOP));
        assert_eq!(origin(6, 23), (LEFT + 23 * 12, TOP + 6 * 12));
    }
}
