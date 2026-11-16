use leptos::prelude::*;

use super::table::data_table;

const WIDTH: f64 = 400.0;
const ROW: f64 = 24.0;
const BAR_HEIGHT: f64 = 10.0;
const LABEL_WIDTH: f64 = 118.0;
const VALUE_WIDTH: f64 = 100.0;
/// The longest label, in characters, before it is cut with an ellipsis.
const LABEL_CHARS: usize = 20;

/// One horizontal bar. `value` sets its length against the chart's `max`, and
/// `text` is what the row says at its end, such as `74%` or `3.2 d`.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub label: String,
    pub value: f64,
    pub text: String,
}

fn bar_area() -> f64 {
    WIDTH - LABEL_WIDTH - VALUE_WIDTH - 8.0
}

/// Each bar's width. A zero, negative, or non-finite value has no bar.
pub fn widths(rows: &[Row], max: f64) -> Vec<f64> {
    rows.iter()
        .map(|row| {
            if max > 0.0 && row.value.is_finite() && row.value > 0.0 {
                (row.value / max).min(1.0) * bar_area()
            } else {
                0.0
            }
        })
        .collect()
}

/// The largest value, or 0 for no rows, for a chart that scales to its data.
pub fn peak(rows: &[Row]) -> f64 {
    rows.iter()
        .map(|row| row.value)
        .filter(|v| v.is_finite())
        .fold(0.0, f64::max)
}

fn cut(label: &str) -> String {
    if label.chars().count() <= LABEL_CHARS {
        return label.to_owned();
    }
    let head: String = label.chars().take(LABEL_CHARS - 1).collect();
    format!("{head}\u{2026}")
}

/// Ranked or bucketed values as horizontal bars. The hidden table holds the
/// same rows.
pub fn hbars(rows: Vec<Row>, max: f64, label: String) -> AnyView {
    let lengths = widths(&rows, max);
    let height = rows.len() as f64 * ROW;
    let marks = rows
        .iter()
        .zip(lengths)
        .enumerate()
        .map(|(i, (row, length))| {
            let top = i as f64 * ROW;
            let mid = top + ROW / 2.0;
            view! {
                <text class="chart-label" x="0" y=format!("{:.1}", mid + 4.0)>
                    <title>{row.label.clone()}</title>
                    {cut(&row.label)}
                </text>
                <rect class="hbar-track" x=format!("{LABEL_WIDTH:.1}") y=format!("{:.1}", mid - BAR_HEIGHT / 2.0) width=format!("{:.1}", bar_area()) height=format!("{BAR_HEIGHT:.1}")/>
                <rect class="hbar" x=format!("{LABEL_WIDTH:.1}") y=format!("{:.1}", mid - BAR_HEIGHT / 2.0) width=format!("{length:.1}") height=format!("{BAR_HEIGHT:.1}")/>
                <text class="chart-label" x=format!("{WIDTH:.1}") y=format!("{:.1}", mid + 4.0) text-anchor="end">
                    {row.text.clone()}
                </text>
            }
        })
        .collect_view();
    let table = rows
        .iter()
        .map(|row| vec![row.label.clone(), row.text.clone()])
        .collect();
    view! {
        <figure class="chart">
            <svg class="chart-svg" viewBox=format!("0 0 {WIDTH} {height}") role="img" aria-label=label.clone()>
                {marks}
            </svg>
            {data_table(label, vec!["Name".to_owned(), "Value".to_owned()], table)}
        </figure>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: f64) -> Row {
        Row {
            label: "acme/alpha".to_owned(),
            value,
            text: format!("{value}"),
        }
    }

    #[test]
    fn no_rows_make_no_bars() {
        assert!(widths(&[], 4.0).is_empty());
        assert_eq!(peak(&[]), 0.0);
    }

    #[test]
    fn one_row_at_the_max_fills_the_track() {
        let got = widths(&[row(5.0)], 5.0);
        assert_eq!(got.len(), 1);
        assert!((got[0] - bar_area()).abs() < 1e-9);
    }

    #[test]
    fn many_rows_scale_with_the_max_and_a_bad_value_has_no_bar() {
        let rows = [row(10.0), row(5.0), row(0.0), row(f64::NAN), row(-1.0)];
        let got = widths(&rows, peak(&rows));
        assert!((got[1] - got[0] / 2.0).abs() < 1e-9);
        assert_eq!(&got[2..], [0.0, 0.0, 0.0]);
        assert_eq!(widths(&rows, 0.0), [0.0; 5]);
    }

    #[test]
    fn a_long_label_is_cut_to_fit() {
        assert_eq!(cut("acme/alpha"), "acme/alpha");
        let long = cut("organisation/a-very-long-repo-name");
        assert_eq!(long.chars().count(), LABEL_CHARS);
        assert!(long.ends_with('\u{2026}'));
    }
}
