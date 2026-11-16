use std::cell::{Cell, RefCell};
use std::sync::Arc;

use crate::store::{use_store, Store};
use domain::ids::Timestamp;
use domain::stats::{Grain, Period};
use domain::trends::{DayCell, HeatLevel};
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;
use web_sys::UrlSearchParams;

use crate::charts::bars::{pair_bars, stacked_bars, Pair, Stack};
use crate::charts::grid::habit_grid;
use crate::charts::hbars::{hbars, peak, Row};
use crate::charts::heatmap::heat_map;
use crate::charts::lines::{lines, Scale, Series, Tone};
use crate::charts::{month_label, month_name, short_date};

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

pub const NOTHING_MERGED: &str = "Nothing merged yet.";
static NULL: Value = Value::Null;

/// A read-only view of one place in the answer. The page reads the answer as
/// JSON, not as the Worker's types, because decoding all of those types costs
/// more of the UI budget than the page itself.
#[derive(Clone, Copy)]
struct J<'a>(&'a Value);

impl<'a> J<'a> {
    /// The member named `key`, or nothing when it is missing.
    fn at(self, key: &str) -> J<'a> {
        J(self.0.get(key).unwrap_or(&NULL))
    }

    fn has(self) -> bool {
        !self.0.is_null()
    }

    fn each(self) -> impl Iterator<Item = J<'a>> {
        self.0.as_array().into_iter().flatten().map(J)
    }

    fn num(self) -> Option<f64> {
        self.0.as_f64().filter(|n| n.is_finite())
    }

    fn n(self) -> u32 {
        self.0
            .as_u64()
            .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
    }

    fn text(self) -> &'a str {
        self.0.as_str().unwrap_or_default()
    }

    fn on(self) -> bool {
        self.0.as_bool().unwrap_or(false)
    }

    fn when(self) -> Timestamp {
        Timestamp::parse_rfc3339(self.text()).unwrap_or(Timestamp(0))
    }
}

const DASH: &str = "\u{2013}";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiError {
    Unauthorized,
    Failed,
}

/// The query string `/api/stats` and `/stats` both take, with no leading `?`.
pub fn period_query(period: Period) -> &'static str {
    match period {
        Period::Last12Months(Grain::Week) => "period=12m&grain=week",
        Period::Last12Months(Grain::Month) => "period=12m&grain=month",
        Period::YearOverYear => "period=yoy",
    }
}

/// The raw JSON, so the page can tell a changed answer from an unchanged one.
async fn fetch_text(period: Period) -> Result<String, ApiError> {
    let response = Request::get(&format!("/api/stats?{}", period_query(period)))
        .send()
        .await
        .map_err(|_| ApiError::Failed)?;
    match response.status() {
        200..=299 => response.text().await.map_err(|_| ApiError::Failed),
        401 => Err(ApiError::Unauthorized),
        _ => Err(ApiError::Failed),
    }
}

fn query(name: &str) -> Option<String> {
    let search = window().location().search().ok()?;
    UrlSearchParams::new_with_str(&search).ok()?.get(name)
}

fn replace_address(to: &str) {
    if let Ok(history) = window().history() {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(to));
    }
}

fn now_secs() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}

thread_local! {
    /// Counts loads, so a late answer to an older load is dropped.
    static LOADS: Cell<u64> = const { Cell::new(0) };
}

/// Starts a load and returns its number.
fn begin_load() -> u64 {
    LOADS.replace(LOADS.get() + 1) + 1
}

/// Whether `load` is still the latest started.
fn is_current(load: u64) -> bool {
    LOADS.get() == load
}

const DEFAULT_PERIOD: Period = Period::Last12Months(Grain::Week);

#[derive(Clone)]
enum Status {
    Loading,
    Ready(Arc<Value>),
    Failed,
}

thread_local! {
    /// The last answer for each period, so a second visit paints at once.
    static ANSWERS: RefCell<Vec<(Period, String)>> = const { RefCell::new(Vec::new()) };
}

fn remembered(period: Period) -> Option<String> {
    ANSWERS.with_borrow(|all| {
        all.iter()
            .find(|(p, _)| *p == period)
            .map(|(_, text)| text.clone())
    })
}

fn remember(period: Period, text: String) {
    ANSWERS.with_borrow_mut(|all| {
        all.retain(|(p, _)| *p != period);
        all.push((period, text));
    });
}

/// The sections every answer carries, each a JSON object. An empty database
/// still sends all of them, so only a malformed answer fails this check.
const SECTIONS: [&str; 8] = [
    "lands", "reviews", "trust", "agents", "output", "waiting", "shape", "habits",
];

fn parse(text: &str) -> Option<Arc<Value>> {
    let value: Value = serde_json::from_str(text).ok()?;
    let shaped = value["yoy_ready"].is_boolean()
        && value["period"].is_object()
        && SECTIONS.iter().all(|key| value[*key].is_object());
    shaped.then(|| Arc::new(value))
}

/// The period the address bar names, or the default for a missing or bad one.
pub fn address_period() -> Period {
    Period::parse(
        query("period").as_deref().unwrap_or("12m"),
        query("grain").as_deref(),
    )
    .unwrap_or(DEFAULT_PERIOD)
}

pub fn href(period: Period) -> String {
    format!("/stats?{}", period_query(period))
}

/// "24 merged in 12 months, 74 percent merge rate", from the S1 totals of the
/// selected period.
fn summary_line(stats: J, period: Period) -> String {
    let lands = stats.at("lands").at("summary");
    let merged = lands.at("merged").n();
    if merged == 0 {
        return NOTHING_MERGED.to_owned();
    }
    let span = if period == Period::YearOverYear {
        "this year"
    } else {
        "in 12 months"
    };
    format!(
        "{merged} merged {span}{}",
        rate_clause(lands.at("merge_rate").num())
    )
}

fn rate_clause(rate: Option<f64>) -> String {
    rate.map(|rate| format!(", {} percent merge rate", whole_percent(rate)))
        .unwrap_or_default()
}

fn whole_percent(share: f64) -> String {
    format!("{:.0}", share * 100.0)
}

/// `74%`, or `None` for a missing or non-finite share.
pub fn percent(share: Option<f64>) -> Option<String> {
    share
        .filter(|s| s.is_finite())
        .map(|s| format!("{}%", whole_percent(s)))
}

/// `45 min`, `3.2 h`, or `2.1 d`, or `None` for a missing or non-finite value.
pub fn duration(hours: Option<f64>) -> Option<String> {
    let hours = hours.filter(|h| h.is_finite() && *h >= 0.0)?;
    let trimmed = |n: f64| format!("{n:.1}").trim_end_matches(".0").to_owned();
    Some(if hours < 1.0 {
        format!("{:.0} min", hours * 60.0)
    } else if hours < 48.0 {
        format!("{} h", trimmed(hours))
    } else {
        format!("{} d", trimmed(hours / 24.0))
    })
}

fn days(days: f64) -> String {
    format!("{} d", format!("{days:.1}").trim_end_matches(".0"))
}

fn weekly(period: Period) -> bool {
    period == Period::Last12Months(Grain::Week)
}

fn size_label(index: usize) -> &'static str {
    [
        "Under 50 lines",
        "50 to 199 lines",
        "200 to 499 lines",
        "500 or more",
    ]
    .get(index)
    .copied()
    .unwrap_or_default()
}

fn stage_name(key: &str) -> &'static str {
    match key {
        "draft" => "Draft",
        "ci_running" => "CI running",
        "ci_failing" => "CI failing",
        "awaiting_review" => "Awaiting review",
        "changes_requested" => "Changes requested",
        "approved" => "Approved",
        _ => "Other",
    }
}

fn milestone_text(kind: &str, repo: &str) -> String {
    match kind {
        "first_merge_in_repo" => format!("First merge in {repo}"),
        "merged50" => "50th merged pull request".to_owned(),
        "merged100" => "100th merged pull request".to_owned(),
        _ => format!("First merge into {repo}, 1,000 or more stars"),
    }
}

fn heat_cells(days: J) -> Vec<DayCell> {
    days.each()
        .map(|day| DayCell {
            date: day.at("date").when(),
            count: day.at("count").n(),
            level: match day.at("level").text() {
                "low" => HeatLevel::Low,
                "mid" => HeatLevel::Mid,
                "high" => HeatLevel::High,
                _ => HeatLevel::None,
            },
        })
        .collect()
}

fn load(period: Period, status: RwSignal<Status>, store: Store) {
    match remembered(period).and_then(|text| parse(&text)) {
        Some(hit) => status.set(Status::Ready(hit)),
        None => status.set(Status::Loading),
    }
    let this_load = begin_load();
    spawn_local(async move {
        let answer = fetch_text(period).await;
        if !is_current(this_load) {
            return;
        }
        // Leaving Stats disposes the signal while the request is in flight.
        let Some(current) = status.try_get_untracked() else {
            return;
        };
        let ready = matches!(current, Status::Ready(_));
        match answer {
            Ok(text) if ready && remembered(period).as_deref() == Some(text.as_str()) => {}
            Ok(text) => match parse(&text) {
                Some(fresh) => {
                    remember(period, text);
                    status.set(Status::Ready(fresh));
                }
                None if !ready => status.set(Status::Failed),
                None => {}
            },
            Err(ApiError::Unauthorized) => store.refresh(),
            Err(ApiError::Failed) if !ready => status.set(Status::Failed),
            Err(ApiError::Failed) => {}
        }
    });
}

fn stat(label: &'static str, value: Option<String>, reason: &'static str) -> impl IntoView {
    match value {
        Some(value) => view! {
            <div class="stat">
                <dt>{label}</dt>
                <dd>{value}</dd>
            </div>
        }
        .into_any(),
        None => view! {
            <div class="stat">
                <dt>{label}</dt>
                <dd>{DASH}</dd>
                <dd class="stat-note">{reason}</dd>
            </div>
        }
        .into_any(),
    }
}

fn count_stat(label: &'static str, value: u32) -> impl IntoView {
    stat(label, Some(value.to_string()), "")
}

fn section(title: &'static str, body: impl IntoView) -> impl IntoView {
    view! {
        <section class="stats-section">
            <h2 class="stats-heading">{title}</h2>
            {body}
        </section>
    }
}

fn chart_title(text: &'static str) -> impl IntoView {
    view! { <h3 class="stats-chart-title">{text}</h3> }
}

fn note(text: &'static str) -> impl IntoView {
    view! { <p class="stats-note">{text}</p> }
}

/// This year and last year as two series, one value per month, from `key` of
/// each `yoy.months` entry.
fn yoy_series(yoy: J, pick: fn(J) -> Option<f64>) -> Vec<Series> {
    let months = |year: &str| yoy.at("months").each().map(|m| pick(m.at(year))).collect();
    vec![
        Series {
            name: "This year",
            tone: Tone::Text,
            values: months("this_year"),
        },
        Series {
            name: "Last year",
            tone: Tone::Faint,
            values: months("last_year"),
        },
    ]
}

fn yoy_labels(yoy: J) -> Vec<String> {
    yoy.at("months")
        .each()
        .map(|m| month_name(i64::from(m.at("month").n())).to_owned())
        .collect()
}

fn gap(yoy: J, key: &str) -> bool {
    yoy.at("gaps").at(key).on()
}

fn lands(lands: J, yoy: J) -> impl IntoView {
    let s = lands.at("summary");
    let cohort_labels: Vec<String> = lands
        .at("months")
        .each()
        .map(|m| month_label(m.at("start").when()))
        .collect();
    let opened: u32 = lands.at("months").each().map(|m| m.at("opened").n()).sum();
    let merged: u32 = lands
        .at("months")
        .each()
        .map(|m| m.at("merged_within_30_days").n())
        .sum();
    let cohort_series = vec![Series {
        name: "Merged within 30 days",
        tone: Tone::Text,
        values: lands
            .at("months")
            .each()
            .map(|m| m.at("share").num())
            .collect(),
    }];
    let compare = yoy.has().then(|| {
        view! {
            {chart_title("Merge rate by month")}
            {lines(yoy_labels(yoy), yoy_series(yoy, |m| m.at("merge_rate").num()), Scale::Percent, "Merge rate by month, this year against last year".to_owned())}
        }
    });
    section(
        "Lands",
        view! {
            <dl class="stats-list">
                {stat("Merge rate", percent(s.at("merge_rate").num()), "No PRs closed yet")}
                {stat("Median to merge", duration(s.at("median_hours_to_merge").num()), "No PRs merged yet")}
                {stat("75th percentile", duration(s.at("p75_hours_to_merge").num()), "No PRs merged yet")}
                {count_stat("Open over 30 days", lands.at("open_over_30_days").n())}
                {stat("Changes requested", percent(s.at("changes_requested_share").num()), "No PRs yet")}
                {stat("CI failed at least once", percent(lands.at("ci_failed_share").num()), "No CI results yet")}
            </dl>
            {(yoy.has() && gap(yoy, "ci_failures")).then(|| note("Last year has no CI results, because the import carries none."))}
            {compare}
            {chart_title("Merged within 30 days, by month opened")}
            {lines(cohort_labels, cohort_series, Scale::Percent, format!("Share of pull requests merged within 30 days, by month opened: {merged} of {opened}"))}
        },
    )
}

fn reviews(r: J, yoy: J) -> impl IntoView {
    // One bar per month, whatever the grain, so the chart stays readable.
    let mut months: Vec<(Timestamp, [u32; 3])> = Vec::new();
    for bucket in r.at("buckets").each() {
        let start = bucket.at("start").when().month_start();
        if months.last().is_none_or(|(at, _)| *at != start) {
            months.push((start, [0; 3]));
        }
        if let Some((_, sums)) = months.last_mut() {
            sums[0] += bucket.at("approved").n();
            sums[1] += bucket.at("changes_requested").n();
            sums[2] += bucket.at("commented").n();
        }
    }
    let labels: Vec<String> = months.iter().map(|(at, _)| month_label(*at)).collect();
    let stack = |name, tone, part: usize| Stack {
        name,
        tone,
        counts: months.iter().map(|(_, sums)| sums[part]).collect(),
    };
    let split = vec![
        stack("Approved", Tone::Text, 0),
        stack("Changes requested", Tone::Muted, 1),
        stack("Commented", Tone::Faint, 2),
    ];
    let total = r.at("total").n();
    let label = format!(
        "Reviews per month: {total} given, {} approved, {} changes requested, {} commented",
        r.at("approved").n(),
        r.at("changes_requested").n(),
        r.at("commented").n()
    );
    let repos: Vec<Row> = r
        .at("top_repos")
        .each()
        .map(|c| Row {
            label: c.at("repo").text().to_owned(),
            value: f64::from(c.at("count").n()),
            text: c.at("count").n().to_string(),
        })
        .collect();
    let max = peak(&repos);
    let compare = yoy.has().then(|| {
        view! {
            {chart_title("Reviews by month")}
            {lines(yoy_labels(yoy), yoy_series(yoy, |m| Some(f64::from(m.at("reviews").n()))), Scale::Count, "Reviews by month, this year against last year".to_owned())}
        }
    });
    let top_label = format!(
        "Repos I review in, {total} reviews across {} repos",
        r.at("repos_reviewed").n()
    );
    section(
        "Reviews",
        view! {
            <dl class="stats-list">
                {count_stat("Reviews given", total)}
                {count_stat("Repos reviewed", r.at("repos_reviewed").n())}
                {count_stat("In other people's repos", r.at("external").n())}
                {count_stat("In my own repos", r.at("own").n())}
            </dl>
            {compare}
            {chart_title("Reviews given")}
            {stacked_bars(labels, split, label)}
            {if repos.is_empty() {
                stat("Repos I review in", None, "No reviews yet").into_any()
            } else {
                view! {
                    {chart_title("Repos I review in")}
                    {hbars(repos, max, top_label)}
                }
                    .into_any()
            }}
        },
    )
}

fn trust(t: J) -> impl IntoView {
    let ranking: Vec<Row> = t
        .at("ranking")
        .each()
        .map(|r| Row {
            label: r.at("repo").text().to_owned(),
            value: f64::from(r.at("merged").n()),
            text: format!(
                "{} \u{b7} {} \u{b7} {}",
                r.at("merged").n(),
                percent(r.at("merge_rate").num()).unwrap_or_else(|| DASH.to_owned()),
                duration(r.at("median_first_response_hours").num())
                    .unwrap_or_else(|| DASH.to_owned())
            ),
        })
        .collect();
    let languages: Vec<Row> = t
        .at("languages")
        .each()
        .map(|l| Row {
            label: l.at("language").text().to_owned(),
            value: f64::from(l.at("merged").n()),
            text: l.at("merged").n().to_string(),
        })
        .collect();
    let rank_label = format!(
        "Repos ranked by merged pull requests, {} repos, with merge rate and first response",
        ranking.len()
    );
    let lang_label = format!(
        "Merged pull requests by language, {} languages",
        languages.len()
    );
    let (rank_max, lang_max) = (peak(&ranking), peak(&languages));
    section(
        "Trust",
        view! {
            <dl class="stats-list">
                {count_stat("New repos", t.at("new_repos").n())}
                {count_stat("Repeat repos", t.at("repeat_repos").n())}
                {count_stat("Merges into 1,000+ stars", t.at("star_merges").n())}
            </dl>
            {if ranking.is_empty() {
                stat("Repo ranking", None, "No merged PRs yet").into_any()
            } else {
                view! {
                    {chart_title("Repos: merged, merge rate, first response")}
                    {hbars(ranking, rank_max, rank_label)}
                }
                    .into_any()
            }}
            {if languages.is_empty() {
                stat("Languages", None, "No merged PRs with a language yet").into_any()
            } else {
                view! {
                    {chart_title("Languages")}
                    {hbars(languages, lang_max, lang_label)}
                }
                    .into_any()
            }}
        },
    )
}

fn agents(a: J) -> impl IntoView {
    let (agent, other) = (a.at("agent"), a.at("other"));
    let both = |pick: fn(J) -> Option<String>| (pick(agent), pick(other));
    let cell = |value: Option<String>| value.unwrap_or_else(|| DASH.to_owned());
    let rows = [
        ("Pull requests", both(|s| Some(s.at("prs").n().to_string()))),
        ("Merge rate", both(|s| percent(s.at("merge_rate").num()))),
        (
            "Median to merge",
            both(|s| duration(s.at("median_hours_to_merge").num())),
        ),
        (
            "Changes requested",
            both(|s| percent(s.at("changes_requested_share").num())),
        ),
    ];
    let body = rows
        .into_iter()
        .map(|(name, (a, o))| {
            view! {
                <tr>
                    <th scope="row">{name}</th>
                    <td>{cell(a)}</td>
                    <td>{cell(o)}</td>
                </tr>
            }
        })
        .collect_view();
    let rates = [("Agent branches", agent), ("Other branches", other)]
        .into_iter()
        .map(|(label, s)| Row {
            label: label.to_owned(),
            value: s.at("merge_rate").num().unwrap_or(0.0),
            text: percent(s.at("merge_rate").num()).unwrap_or_else(|| DASH.to_owned()),
        })
        .collect::<Vec<_>>();
    let label = format!(
        "Merge rate, agent branches {}, other branches {}",
        percent(agent.at("merge_rate").num()).unwrap_or_else(|| "none yet".to_owned()),
        percent(other.at("merge_rate").num()).unwrap_or_else(|| "none yet".to_owned())
    );
    section(
        "Agents",
        view! {
            {if agent.at("prs").n() == 0 {
                stat("Agent branches", None, "No agent PRs yet").into_any()
            } else {
                view! {
                    <table class="stats-compare">
                        <thead>
                            <tr>
                                <td></td>
                                <th scope="col">"Agent branches"</th>
                                <th scope="col">"Other branches"</th>
                            </tr>
                        </thead>
                        <tbody>{body}</tbody>
                    </table>
                    {chart_title("Merge rate")}
                    {hbars(rates, 1.0, label)}
                }
                    .into_any()
            }}
        },
    )
}

fn best_text(best: J, month: bool) -> Option<String> {
    best.has().then(|| {
        let at = best.at("start").when();
        let when = if month {
            month_label(at)
        } else {
            short_date(at)
        };
        format!("{when}, {} merged", best.at("merged").n())
    })
}

fn output(o: J, period: Period, yoy: J) -> impl IntoView {
    let month = !weekly(period);
    let pairs: Vec<Pair> = o
        .at(if month { "months" } else { "weeks" })
        .each()
        .map(|p| Pair {
            start: p.at("start").when(),
            opened: p.at("opened").n(),
            merged: p.at("merged").n(),
        })
        .collect();
    let average = o
        .at("weeks")
        .each()
        .last()
        .map(|w| format!("{:.1} a week", w.at("rolling_merged").num().unwrap_or(0.0)));
    let chart = if yoy.has() {
        view! {
            {chart_title("Merged by month")}
            {lines(yoy_labels(yoy), yoy_series(yoy, |m| Some(f64::from(m.at("merged").n()))), Scale::Count, "Pull requests merged by month, this year against last year".to_owned())}
        }
        .into_any()
    } else {
        view! {
            {chart_title(if month { "Opened and merged by month" } else { "Opened and merged by week" })}
            {pair_bars(pairs, if month { "month" } else { "week" })}
        }
        .into_any()
    };
    section(
        "Output",
        view! {
            <dl class="stats-list">
                {stat("Best week", best_text(o.at("best_week"), false), "No merges yet")}
                {stat("Best month", best_text(o.at("best_month"), true), "No merges yet")}
                {stat("4 week average", average, "No weeks yet")}
                {count_stat("Streak of weeks", o.at("current_streak").n())}
                {count_stat("Longest streak", o.at("longest_streak").n())}
                {count_stat("Repos in 52 weeks", o.at("repos_total").n())}
            </dl>
            {chart}
            {chart_title("Activity per day")}
            {heat_map(heat_cells(o.at("heatmap")))}
        },
    )
}

fn waiting(w: J, yoy: J) -> impl IntoView {
    let rows: Vec<Row> = w
        .at("stages")
        .each()
        .map(|s| Row {
            label: stage_name(s.at("stage").text()).to_owned(),
            value: s.at("median_hours").num().unwrap_or(0.0),
            text: duration(s.at("median_hours").num()).unwrap_or_else(|| DASH.to_owned()),
        })
        .collect();
    let any = w
        .at("stages")
        .each()
        .any(|s| s.at("median_hours").num().is_some());
    let max = peak(&rows);
    let prs = w.at("prs").n();
    let label = format!("Median time in each stage, {prs} pull requests with stage history");
    let days = |key: &str| (prs > 0).then(|| days(w.at(key).num().unwrap_or(0.0)));
    section(
        "Waiting",
        view! {
            <dl class="stats-list">
                {stat("Waiting on maintainers", days("days_waiting_on_them"), "No history yet")}
                {stat("Waiting on me", days("days_waiting_on_me"), "No history yet")}
            </dl>
            {if any {
                view! {
                    {chart_title("Median time in each stage")}
                    {hbars(rows, max, label)}
                }
                    .into_any()
            } else {
                stat("Stage times", None, "History starts the day it ships").into_any()
            }}
            {(yoy.has() && (gap(yoy, "stage_times") || gap(yoy, "turn_times"))).then(|| note("Last year has no stage or turn history."))}
        },
    )
}

fn shape(s: J) -> impl IntoView {
    let rows: Vec<Row> = s
        .at("buckets")
        .each()
        .enumerate()
        .map(|(i, b)| {
            let (rate, prs) = (
                b.at("summary").at("merge_rate").num(),
                b.at("summary").at("prs").n(),
            );
            Row {
                label: size_label(i).to_owned(),
                value: rate.unwrap_or(0.0),
                text: match (percent(rate), prs) {
                    (Some(rate), n) => format!("{rate} \u{b7} {n}"),
                    (None, 0) => DASH.to_owned(),
                    (None, n) => format!("{DASH} \u{b7} {n}"),
                },
            }
        })
        .collect();
    let any = s
        .at("buckets")
        .each()
        .any(|b| b.at("summary").at("merge_rate").num().is_some());
    let label = format!("Merge rate by size, {} size groups", rows.len());
    section(
        "Shape",
        view! {
            <dl class="stats-list">{count_stat("Issues closed by my PRs", s.at("issues_closed").n())}</dl>
            {if any {
                view! {
                    {chart_title("Merge rate and PRs, by lines changed")}
                    {hbars(rows, 1.0, label)}
                }
                    .into_any()
            } else {
                stat("Merge rate by size", None, "No PRs closed yet").into_any()
            }}
        },
    )
}

fn habits(h: J, yoy: J) -> impl IntoView {
    let goal = h.at("goal").has().then(|| {
        let g = h.at("goal");
        let rows = vec![Row {
            label: "Merged this month".to_owned(),
            value: g.at("progress").num().unwrap_or(0.0).min(1.0),
            text: format!(
                "{} of {}",
                g.at("merged_this_month").n(),
                g.at("target").n()
            ),
        }];
        let label = format!(
            "Progress toward the monthly goal, {} of {} merged",
            g.at("merged_this_month").n(),
            g.at("target").n()
        );
        view! {
            {chart_title("Monthly goal")}
            {hbars(rows, 1.0, label)}
        }
    });
    let milestones = h
        .at("milestones")
        .each()
        .map(|m| {
            let text = milestone_text(m.at("kind").text(), m.at("repo").text());
            view! {
                <li>
                    <span>{text}</span>
                    <span class="stats-when">{short_date(m.at("at").when())}</span>
                </li>
            }
        })
        .collect_view();
    let none = h.at("milestones").each().next().is_none();
    let events = h.at("events").n();
    let grid: Vec<Vec<u32>> = h
        .at("grid")
        .each()
        .map(|row| row.each().map(|c| c.n()).collect())
        .collect();
    section(
        "Habits",
        view! {
            {if events == 0 {
                stat("When I work", None, "No activity of mine yet").into_any()
            } else {
                view! {
                    {chart_title("When I work")}
                    {habit_grid(grid, format!("Activity by weekday and hour, {events} events"))}
                }
                    .into_any()
            }}
            {(yoy.has() && gap(yoy, "habits")).then(|| note("Last year has no activity history."))}
            {goal}
            {if none {
                stat("Milestones", None, "None reached in this period").into_any()
            } else {
                view! {
                    {chart_title("Milestones")}
                    <ul class="stats-milestones">{milestones}</ul>
                }
                    .into_any()
            }}
        },
    )
}

fn body(stats: &Value, period: Period) -> impl IntoView {
    let s = J(stats);
    let yoy = s.at("yoy");
    view! {
        <h1 class="stats-summary">{summary_line(s, period)}</h1>
        <div class="stats-grid">
            {lands(s.at("lands"), yoy)}
            {reviews(s.at("reviews"), yoy)}
            {trust(s.at("trust"))}
            {agents(s.at("agents"))}
            {output(s.at("output"), period, yoy)}
            {waiting(s.at("waiting"), yoy)}
            {shape(s.at("shape"))}
            {habits(s.at("habits"), yoy)}
        </div>
    }
}

fn option(
    name: &'static str,
    text: &'static str,
    checked: impl Fn() -> bool + Send + Sync + 'static,
    pick: impl Fn() + 'static,
) -> impl IntoView {
    view! {
        <label class="switch-option">
            <input type="radio" name=name prop:checked=checked on:change=move |_| pick()/>
            <span>{text}</span>
        </label>
    }
}

/// The Stats page (F14): eight sections over a period the switch picks.
#[component]
pub fn Stats() -> impl IntoView {
    let store = use_store();
    let period = RwSignal::new(address_period());
    let status = RwSignal::new(Status::Loading);
    let yoy_ready = RwSignal::new(None::<bool>);
    Effect::new(move |_| load(period.get(), status, store));
    Effect::new(move |_| {
        if let Status::Ready(stats) = status.get() {
            yoy_ready.set(Some(J(&stats).at("yoy_ready").on()));
        }
    });
    let choose = move |next: Period| {
        period.set(next);
        replace_address(&href(next));
    };
    let is_yoy = move || period.get() == Period::YearOverYear;
    let grain_is = move |grain: Grain| period.get() == Period::Last12Months(grain);
    view! {
        <section class="stats">
            <div class="switch-row">
                <fieldset class="switch">
                    <legend class="sr-only">"Period"</legend>
                    {option("period", "Last 12 months", move || !is_yoy(), move || {
                        if is_yoy() {
                            choose(DEFAULT_PERIOD);
                        }
                    })}
                    {move || {
                        (yoy_ready.get() == Some(true) || is_yoy())
                            .then(|| {
                                option("period", "This year vs last year", is_yoy, move || choose(Period::YearOverYear))
                            })
                    }}
                </fieldset>
                {move || {
                    (!is_yoy())
                        .then(|| {
                            view! {
                                <fieldset class="switch">
                                    <legend class="sr-only">"Group by"</legend>
                                    {option("grain", "Weekly", move || grain_is(Grain::Week), move || {
                                        choose(Period::Last12Months(Grain::Week))
                                    })}
                                    {option("grain", "Monthly", move || grain_is(Grain::Month), move || {
                                        choose(Period::Last12Months(Grain::Month))
                                    })}
                                </fieldset>
                            }
                        })
                }}
            </div>
            {move || {
                (yoy_ready.get() == Some(false))
                    .then(|| {
                        let year = Timestamp(now_secs()).year_month().0 + 1;
                        view! {
                            <p class="stats-note">
                                {format!(
                                    "This year vs last year unlocks when the history import is done and last year is complete, on 1 Jan {year} at the latest."
                                )}
                            </p>
                        }
                    })
            }}
            {move || match status.get() {
                Status::Loading => view! { <p class="text-muted">"Loading."</p> }.into_any(),
                Status::Failed => {
                    view! {
                        <p class="text-muted" role="alert">
                            "Could not load stats. Reload to try again."
                        </p>
                    }
                        .into_any()
                }
                Status::Ready(stats) => body(&stats, period.get()).into_any(),
            }}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use domain::stage::Stage;
    use domain::stats::{build_stats, ImportState, MilestoneKind, StatsInput};
    use domain::trends::build_trends;
    use serde_json::json;

    use super::*;

    const NOW: i64 = 1_794_830_400;

    fn empty(period: Period) -> Value {
        let now = Timestamp(NOW);
        let input = StatsInput {
            now,
            utc_offset_minutes: 0,
            goal_merged_per_month: None,
            import: ImportState::default(),
            prs: Vec::new(),
            merges: Vec::new(),
            reviews: Vec::new(),
            transitions: Vec::new(),
            closed_issues: 0,
            events: Vec::new(),
            trends: build_trends(&[], &[], &[], now),
        };
        serde_json::to_value(build_stats(&input, period)).expect("stats serialize")
    }

    fn lands(merged: u32, rate: Option<f64>) -> Value {
        json!({ "lands": { "summary": { "merged": merged, "merge_rate": rate } } })
    }

    #[test]
    fn the_summary_says_nothing_merged_for_zero() {
        let stats = empty(DEFAULT_PERIOD);
        assert_eq!(
            summary_line(J(&stats), DEFAULT_PERIOD),
            "Nothing merged yet."
        );
        let yoy = empty(Period::YearOverYear);
        assert_eq!(
            summary_line(J(&yoy), Period::YearOverYear),
            "Nothing merged yet."
        );
    }

    #[test]
    fn the_summary_follows_the_lands_totals_and_drops_a_null_rate() {
        let many = lands(24, Some(0.74));
        assert_eq!(
            summary_line(J(&many), DEFAULT_PERIOD),
            "24 merged in 12 months, 74 percent merge rate"
        );
        let one = lands(1, None);
        assert_eq!(
            summary_line(J(&one), DEFAULT_PERIOD),
            "1 merged in 12 months"
        );
        assert_eq!(
            summary_line(J(&one), Period::YearOverYear),
            "1 merged this year"
        );
    }

    #[test]
    fn a_missing_number_is_a_dash_not_a_zero() {
        assert_eq!(percent(None), None);
        assert_eq!(percent(Some(f64::NAN)), None);
        assert_eq!(percent(Some(0.0)).as_deref(), Some("0%"));
        assert_eq!(percent(Some(0.746)).as_deref(), Some("75%"));
        assert_eq!(duration(None), None);
        assert_eq!(duration(Some(f64::INFINITY)), None);
        assert_eq!(duration(Some(0.25)).as_deref(), Some("15 min"));
        assert_eq!(duration(Some(3.25)).as_deref(), Some("3.2 h"));
        assert_eq!(duration(Some(24.0)).as_deref(), Some("24 h"));
        assert_eq!(duration(Some(72.0)).as_deref(), Some("3 d"));
        assert_eq!(days(0.0), "0 d");
        assert_eq!(days(2.5), "2.5 d");
    }

    #[test]
    fn the_reader_gives_nothing_for_a_missing_or_wrong_value() {
        let v = json!({ "a": { "n": 3, "f": 0.5, "t": "x", "on": true }, "nil": null });
        let j = J(&v);
        assert_eq!(j.at("a").at("n").n(), 3);
        assert_eq!(j.at("a").at("f").num(), Some(0.5));
        assert_eq!(j.at("a").at("t").text(), "x");
        assert!(j.at("a").at("on").on());
        assert!(!j.at("nil").has() && !j.at("missing").has());
        assert_eq!(j.at("missing").at("deeper").n(), 0);
        assert_eq!(j.at("a").at("t").num(), None);
        assert_eq!(j.at("a").at("n").each().count(), 0);
    }

    #[test]
    fn an_empty_answer_reads_as_dashes_and_no_rows() {
        let stats = empty(DEFAULT_PERIOD);
        let s = J(&stats);
        assert_eq!(
            percent(s.at("lands").at("summary").at("merge_rate").num()),
            None
        );
        assert_eq!(s.at("reviews").at("top_repos").each().count(), 0);
        assert_eq!(s.at("habits").at("milestones").each().count(), 0);
        assert!(!s.at("habits").at("goal").has());
        assert!(!s.at("yoy").has());
    }

    #[test]
    fn periods_round_trip_through_the_address() {
        for period in [
            Period::Last12Months(Grain::Week),
            Period::Last12Months(Grain::Month),
            Period::YearOverYear,
        ] {
            let q = period_query(period);
            let mut parts = q.split('&').map(|p| p.split_once('=').expect("pair"));
            let (_, kind) = parts.next().expect("period");
            let grain = parts.next().map(|(_, g)| g);
            assert_eq!(Period::parse(kind, grain), Some(period));
        }
        assert_eq!(href(Period::YearOverYear), "/stats?period=yoy");
    }

    #[test]
    fn the_page_reads_the_names_the_worker_writes() {
        for (kind, text) in [
            (MilestoneKind::FirstMergeInRepo, "First merge in r"),
            (MilestoneKind::Merged50, "50th merged pull request"),
            (MilestoneKind::Merged100, "100th merged pull request"),
            (
                MilestoneKind::FirstStarMerge,
                "First merge into r, 1,000 or more stars",
            ),
        ] {
            let key = serde_json::to_value(kind).expect("kind");
            assert_eq!(milestone_text(key.as_str().expect("text"), "r"), text);
        }
        for stage in [
            Stage::CiRunning,
            Stage::CiFailing,
            Stage::AwaitingReview,
            Stage::ChangesRequested,
            Stage::Approved,
        ] {
            let key = serde_json::to_value(stage).expect("stage");
            assert_ne!(stage_name(key.as_str().expect("text")), "Other");
        }
        assert_eq!(size_label(3), "500 or more");
        assert_eq!(size_label(9), "");
        let cell = json!({ "date": "2026-10-14T00:00:00Z", "count": 2, "level": "high" });
        let cells = heat_cells(J(&json!([cell])));
        assert_eq!((cells[0].count, cells[0].level), (2, HeatLevel::High));
        assert_eq!(
            cells[0].date,
            Timestamp::parse_rfc3339("2026-10-14T00:00:00Z").expect("ts")
        );
    }

    #[test]
    fn the_last_answer_per_period_is_kept() {
        assert!(remembered(DEFAULT_PERIOD).is_none());
        remember(DEFAULT_PERIOD, "a".to_owned());
        remember(DEFAULT_PERIOD, "b".to_owned());
        assert_eq!(remembered(DEFAULT_PERIOD).as_deref(), Some("b"));
        assert!(remembered(Period::YearOverYear).is_none());
        assert_eq!(month_label(Timestamp(NOW)), "Nov");
        assert!(parse("not json").is_none());
    }

    #[test]
    fn only_a_stats_shaped_answer_parses() {
        for bad in ["{}", "null", "[]", r#"{"yoy_ready":"no"}"#] {
            assert!(parse(bad).is_none(), "{bad}");
        }
        let mut shaped = json!({ "period": { "kind": "year_over_year" }, "yoy_ready": false });
        for key in SECTIONS {
            shaped[key] = json!({});
        }
        assert!(parse(&shaped.to_string()).is_some());
        shaped["lands"] = json!(null);
        assert!(parse(&shaped.to_string()).is_none());
    }

    #[test]
    fn only_the_latest_load_is_current() {
        let first = begin_load();
        assert!(is_current(first));
        let second = begin_load();
        let third = begin_load();
        assert!(!is_current(first) && !is_current(second));
        assert!(is_current(third));
    }
}
