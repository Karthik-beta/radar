use domain::trends::nice_max;
use leptos::prelude::*;

use super::axis_slots;
use super::table::data_table;

const WIDTH: f64 = 400.0;
const HEIGHT: f64 = 160.0;
const LEFT: f64 = 36.0;
const RIGHT: f64 = 6.0;
const TOP: f64 = 8.0;
const BOTTOM: f64 = 20.0;
/// Up to this many points get a dot, so a lone point still shows.
const DOTS_UP_TO: usize = 13;

/// Grey only (ADR 0003): the main series, a second, and the quietest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Text,
    Muted,
    Faint,
}

impl Tone {
    fn class(self) -> &'static str {
        match self {
            Tone::Text => "line-text",
            Tone::Muted => "line-muted",
            Tone::Faint => "line-faint",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    /// Counts, with a ceiling from `nice_max`.
    Count,
    /// Shares from 0 to 1, drawn as percent.
    Percent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub name: &'static str,
    pub tone: Tone,
    /// `None` leaves a gap.
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub ceiling: f64,
    pub lines: Vec<Vec<Option<(f64, f64)>>>,
}

fn plot_height() -> f64 {
    HEIGHT - TOP - BOTTOM
}

fn baseline() -> f64 {
    TOP + plot_height()
}

fn x_at(index: usize, count: usize) -> f64 {
    let plot = WIDTH - LEFT - RIGHT;
    match count {
        0 | 1 => LEFT + plot / 2.0,
        n => LEFT + index as f64 * plot / (n - 1) as f64,
    }
}

/// Every point of every series on one scale. Series may differ in length, and
/// the longest sets the x spacing.
pub fn layout(series: &[Series], scale: Scale) -> Layout {
    let count = series.iter().map(|s| s.values.len()).max().unwrap_or(0);
    let peak = series
        .iter()
        .flat_map(|s| s.values.iter().flatten())
        .copied()
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max);
    let ceiling = match scale {
        Scale::Percent => 1.0,
        Scale::Count => f64::from(nice_max(peak.ceil() as u32)),
    };
    let lines = series
        .iter()
        .map(|s| {
            s.values
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    v.filter(|v| v.is_finite()).map(|v| {
                        let share = (v / ceiling).clamp(0.0, 1.0);
                        (x_at(i, count), baseline() - share * plot_height())
                    })
                })
                .collect()
        })
        .collect();
    Layout { ceiling, lines }
}

/// One `M` per unbroken run, so a gap is a gap.
fn path(points: &[Option<(f64, f64)>]) -> String {
    let mut d = String::new();
    let mut pen_down = false;
    for point in points {
        match point {
            Some((x, y)) => {
                d.push_str(&format!(
                    "{}{x:.1} {y:.1} ",
                    if pen_down { "L" } else { "M" }
                ));
                pen_down = true;
            }
            None => pen_down = false,
        }
    }
    d.trim_end().to_owned()
}

pub fn cell(value: Option<f64>, scale: Scale) -> String {
    match (value.filter(|v| v.is_finite()), scale) {
        (None, _) => "-".to_owned(),
        (Some(v), Scale::Percent) => format!("{:.0}%", v * 100.0),
        (Some(v), Scale::Count) if (v - v.round()).abs() < 1e-9 => format!("{v:.0}"),
        (Some(v), Scale::Count) => format!("{v:.1}"),
    }
}

/// One or more series over shared x labels. The hidden table holds every value.
pub fn lines(labels: Vec<String>, series: Vec<Series>, scale: Scale, label: String) -> AnyView {
    let chart = layout(&series, scale);
    let count = labels.len();
    let top = match scale {
        Scale::Percent => "100%".to_owned(),
        Scale::Count => cell(Some(chart.ceiling), scale),
    };
    let grid = [0.0, 0.5, 1.0]
        .into_iter()
        .map(|share| baseline() - share * plot_height())
        .map(|y| {
            view! {
                <line
                    class="chart-grid"
                    x1=format!("{LEFT:.1}")
                    x2=format!("{:.1}", WIDTH - RIGHT)
                    y1=format!("{y:.1}")
                    y2=format!("{y:.1}")
                />
            }
        })
        .collect_view();
    let ticks = [(0.0, "0".to_owned()), (1.0, top)]
        .into_iter()
        .map(|(share, text)| {
            let y = baseline() - share * plot_height() + 4.0;
            view! {
                <text class="chart-label" x=format!("{:.1}", LEFT - 4.0) y=format!("{y:.1}") text-anchor="end">
                    {text}
                </text>
            }
        })
        .collect_view();
    let last = count.saturating_sub(1);
    let axis = axis_slots(count)
        .into_iter()
        .filter_map(|i| labels.get(i).map(|text| (i, text.clone())))
        .map(|(i, text)| {
            let anchor = match i {
                0 => "start",
                i if i == last => "end",
                _ => "middle",
            };
            view! {
                <text class="chart-label" x=format!("{:.1}", x_at(i, count)) y=format!("{:.1}", HEIGHT - 4.0) text-anchor=anchor>
                    {text}
                </text>
            }
        })
        .collect_view();
    let drawn = series
        .iter()
        .zip(&chart.lines)
        .map(|(s, points)| {
            let class = s.tone.class();
            let dots = (count <= DOTS_UP_TO).then(|| {
                points
                    .iter()
                    .flatten()
                    .map(|(x, y)| {
                        view! { <circle class=format!("{class}-dot") cx=format!("{x:.1}") cy=format!("{y:.1}") r="2"/> }
                    })
                    .collect_view()
            });
            view! {
                <path class=format!("chart-line {class}") d=path(points)/>
                {dots}
            }
        })
        .collect_view();
    let keys = series
        .iter()
        .map(|s| {
            view! { <span><i class=format!("chart-key {}-key", s.tone.class())></i>{s.name}</span> }
        })
        .collect_view();
    let head = std::iter::once("Period".to_owned())
        .chain(series.iter().map(|s| s.name.to_owned()))
        .collect();
    let rows = labels
        .iter()
        .enumerate()
        .map(|(i, text)| {
            std::iter::once(text.clone())
                .chain(
                    series
                        .iter()
                        .map(|s| cell(s.values.get(i).copied().flatten(), scale)),
                )
                .collect()
        })
        .collect();
    view! {
        <figure class="chart">
            <svg class="chart-svg" viewBox=format!("0 0 {WIDTH} {HEIGHT}") role="img" aria-label=label.clone()>
                {grid}
                {ticks}
                {drawn}
                {axis}
            </svg>
            <figcaption class="chart-legend">{keys}</figcaption>
            {data_table(label, head, rows)}
        </figure>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(values: &[Option<f64>]) -> Vec<Series> {
        vec![Series {
            name: "Merged",
            tone: Tone::Text,
            values: values.to_vec(),
        }]
    }

    fn marks(chart: &Layout) -> usize {
        chart.lines.iter().flatten().flatten().count()
    }

    #[test]
    fn an_empty_chart_has_no_marks_and_a_floor_ceiling() {
        let chart = layout(&[], Scale::Count);
        assert_eq!((marks(&chart), chart.ceiling), (0, 4.0));
        assert_eq!(marks(&layout(&one(&[]), Scale::Count)), 0);
    }

    #[test]
    fn one_point_sits_in_the_middle() {
        let chart = layout(&one(&[Some(2.0)]), Scale::Count);
        assert_eq!(marks(&chart), 1);
        let (x, y) = chart.lines[0][0].expect("point");
        assert!((x - (LEFT + (WIDTH - LEFT - RIGHT) / 2.0)).abs() < 1e-9);
        assert!((y - (baseline() - 0.5 * plot_height())).abs() < 1e-9);
    }

    #[test]
    fn many_points_span_the_plot_and_scale_with_nice_max() {
        let values: Vec<_> = (0..52).map(|i| Some(f64::from(i % 8))).collect();
        let chart = layout(&one(&values), Scale::Count);
        assert_eq!((marks(&chart), chart.ceiling), (52, 10.0));
        let first = chart.lines[0][0].expect("first");
        let last = chart.lines[0][51].expect("last");
        assert!((first.0 - LEFT).abs() < 1e-9 && (last.0 - (WIDTH - RIGHT)).abs() < 1e-9);
        let top = layout(&one(&[Some(10.0)]), Scale::Count).lines[0][0].expect("top");
        assert!((top.1 - TOP).abs() < 1e-9);
    }

    #[test]
    fn a_gap_breaks_the_path_and_a_percent_scale_ends_at_one() {
        let points = layout(
            &one(&[Some(0.5), None, Some(1.0), Some(0.25)]),
            Scale::Percent,
        );
        assert_eq!(points.ceiling, 1.0);
        assert_eq!(marks(&points), 3);
        let d = path(&points.lines[0]);
        assert_eq!(d.matches('M').count(), 2);
        assert_eq!(d.matches('L').count(), 1);
    }

    #[test]
    fn a_missing_value_reads_as_a_dash() {
        assert_eq!(cell(None, Scale::Percent), "-");
        assert_eq!(cell(Some(f64::NAN), Scale::Count), "-");
        assert_eq!(cell(Some(0.74), Scale::Percent), "74%");
        assert_eq!(cell(Some(3.0), Scale::Count), "3");
        assert_eq!(cell(Some(2.25), Scale::Count), "2.2");
    }
}
