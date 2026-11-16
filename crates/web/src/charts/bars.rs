use domain::ids::Timestamp;
use domain::trends::nice_max;
use leptos::prelude::*;

use super::{axis_slots, iso_date, month_label, short_date};

const WIDTH: f64 = 400.0;
const HEIGHT: f64 = 160.0;
const LEFT: f64 = 24.0;
const RIGHT: f64 = 4.0;
const TOP: f64 = 8.0;
const BOTTOM: f64 = 20.0;

/// Opened and merged counts for one week or month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pair {
    pub start: Timestamp,
    pub opened: u32,
    pub merged: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub ceiling: u32,
    pub opened: Vec<Option<Bar>>,
    pub merged: Vec<Option<Bar>>,
}

fn plot_height() -> f64 {
    HEIGHT - TOP - BOTTOM
}

pub fn baseline() -> f64 {
    TOP + plot_height()
}

fn slot_width(count: usize) -> f64 {
    (WIDTH - LEFT - RIGHT) / count.max(1) as f64
}

/// Two bars per slot, side by side. A slot with a zero count has no bar, so an
/// empty chart is just its axes.
pub fn layout(weeks: &[Pair]) -> Layout {
    let peak = weeks
        .iter()
        .map(|w| w.opened.max(w.merged))
        .max()
        .unwrap_or(0);
    let ceiling = nice_max(peak);
    let slot = slot_width(weeks.len());
    let width = (slot - 0.5) / 2.0;
    let bar = |index: usize, side: f64, value: u32| {
        (value > 0).then(|| {
            let height = f64::from(value) / f64::from(ceiling) * plot_height();
            Bar {
                x: LEFT + index as f64 * slot + 0.25 + side * width,
                y: baseline() - height,
                width,
                height,
            }
        })
    };
    Layout {
        ceiling,
        opened: weeks
            .iter()
            .enumerate()
            .map(|(i, w)| bar(i, 0.0, w.opened))
            .collect(),
        merged: weeks
            .iter()
            .enumerate()
            .map(|(i, w)| bar(i, 1.0, w.merged))
            .collect(),
    }
}

fn rects(bars: &[Option<Bar>], class: &'static str) -> impl IntoView {
    bars.iter()
        .flatten()
        .map(|bar| {
            view! {
                <rect
                    class=class
                    x=format!("{:.2}", bar.x)
                    y=format!("{:.2}", bar.y)
                    width=format!("{:.2}", bar.width)
                    height=format!("{:.2}", bar.height)
                />
            }
        })
        .collect_view()
}

/// Pull requests opened and merged per week or month (`unit`). The hidden table
/// beside it holds the same numbers for a screen reader.
#[component]
pub fn PairBars(weeks: Vec<Pair>, unit: &'static str) -> impl IntoView {
    let chart = layout(&weeks);
    let opened_total: u32 = weeks.iter().map(|w| w.opened).sum();
    let merged_total: u32 = weeks.iter().map(|w| w.merged).sum();
    let label = format!(
        "Pull requests per {unit} over {} {unit}s, {opened_total} opened and {merged_total} merged",
        weeks.len()
    );
    let date = move |at: Timestamp| {
        if unit == "month" {
            month_label(at)
        } else {
            short_date(at)
        }
    };
    let slot = slot_width(weeks.len());
    let grid = [0.0, 0.5, 1.0]
        .into_iter()
        .map(|share| baseline() - share * plot_height())
        .collect::<Vec<_>>();
    let ticks = [(0.0, 0u32), (1.0, chart.ceiling)]
        .into_iter()
        .map(|(share, value)| {
            let y = baseline() - share * plot_height();
            view! {
                <text class="chart-label" x=format!("{:.2}", LEFT - 4.0) y=format!("{:.2}", y + 4.0) text-anchor="end">
                    {value}
                </text>
            }
        })
        .collect_view();
    let last = weeks.len().saturating_sub(1);
    let dates = axis_slots(weeks.len())
        .into_iter()
        .filter_map(|i| weeks.get(i).map(|w| (i, w)))
        .map(|(i, week)| {
            let x = LEFT + i as f64 * slot;
            let anchor = match i {
                0 => "start",
                i if i == last => "end",
                _ => "middle",
            };
            view! {
                <text class="chart-label" x=format!("{x:.2}") y=format!("{:.2}", HEIGHT - 4.0) text-anchor=anchor>
                    {date(week.start)}
                </text>
            }
        })
        .collect_view();
    view! {
        <figure class="chart">
            <svg
                class="chart-svg"
                viewBox=format!("0 0 {WIDTH} {HEIGHT}")
                role="img"
                aria-label=label
            >
                {grid
                    .into_iter()
                    .map(|y| {
                        view! {
                            <line
                                class="chart-grid"
                                x1=format!("{LEFT:.2}")
                                x2=format!("{:.2}", WIDTH - RIGHT)
                                y1=format!("{y:.2}")
                                y2=format!("{y:.2}")
                            />
                        }
                    })
                    .collect_view()}
                {ticks}
                {rects(&chart.opened, "chart-opened")}
                {rects(&chart.merged, "chart-merged")}
                {dates}
            </svg>
            <figcaption class="chart-legend">
                <span><i class="chart-key chart-key-opened"></i>"Opened"</span>
                <span><i class="chart-key chart-key-merged"></i>"Merged"</span>
            </figcaption>
            <table class="sr-only">
                <caption>{format!("Pull requests per {unit}")}</caption>
                <thead>
                    <tr>
                        <th scope="col">{if unit == "month" { "Month of" } else { "Week of" }}</th>
                        <th scope="col">"Opened"</th>
                        <th scope="col">"Merged"</th>
                    </tr>
                </thead>
                <tbody>
                    {weeks
                        .iter()
                        .map(|w| {
                            view! {
                                <tr>
                                    <th scope="row">{iso_date(w.start)}</th>
                                    <td>{w.opened}</td>
                                    <td>{w.merged}</td>
                                </tr>
                            }
                        })
                        .collect_view()}
                </tbody>
            </table>
        </figure>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weeks(counts: &[(u32, u32)]) -> Vec<Pair> {
        counts
            .iter()
            .enumerate()
            .map(|(i, (opened, merged))| Pair {
                start: Timestamp(345_600 + i as i64 * 604_800),
                opened: *opened,
                merged: *merged,
            })
            .collect()
    }

    #[test]
    fn one_pair_makes_one_bar_per_side() {
        let chart = layout(&weeks(&[(3, 1)]));
        assert_eq!(chart.opened.iter().flatten().count(), 1);
        assert_eq!(chart.merged.iter().flatten().count(), 1);
        assert_eq!(chart.ceiling, 4);
    }

    #[test]
    fn an_empty_chart_has_a_floor_ceiling_and_no_bars() {
        let chart = layout(&weeks(&[(0, 0); 52]));
        assert_eq!(chart.ceiling, 4);
        assert!(chart
            .opened
            .iter()
            .chain(&chart.merged)
            .all(Option::is_none));
    }

    #[test]
    fn bar_height_is_its_share_of_the_ceiling() {
        let chart = layout(&weeks(&[(2, 4), (5, 0)]));
        assert_eq!(chart.ceiling, 5);
        let full = chart.opened[1].expect("bar");
        assert!((full.height - plot_height()).abs() < 1e-9);
        assert!((full.y - TOP).abs() < 1e-9);
        let merged = chart.merged[0].expect("bar");
        assert!((merged.height - plot_height() * 0.8).abs() < 1e-9);
        assert!(chart.merged[1].is_none());
    }

    #[test]
    fn bars_stay_inside_the_plot() {
        let chart = layout(&weeks(&[(9, 9); 52]));
        let last = chart.merged[51].expect("bar");
        assert!(last.x + last.width <= WIDTH - RIGHT + 1e-9);
        let first = chart.opened[0].expect("bar");
        assert!(first.x >= LEFT);
    }
}
