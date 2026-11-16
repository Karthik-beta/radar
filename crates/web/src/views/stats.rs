use std::cell::RefCell;

use domain::ids::Timestamp;
use domain::stats::{
    Best, Grain, Habits, Lands, MilestoneKind, Output, Period, Reviews, Shape, SizeClass,
    Stats as StatsData, Summary, Trust, Waiting, Yoy,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::api::{self, period_query, ApiError};
use crate::charts::bars::{Pair, PairBars};
use crate::charts::grid::Grid;
use crate::charts::hbars::{peak, HBars, Row};
use crate::charts::heatmap::HeatMap;
use crate::charts::lines::{Lines, Scale, Series, Tone};
use crate::charts::{month_label, month_name, short_date};
use crate::route::{query, use_location};
use crate::stage_label::stage_label;
use crate::store::{use_store, Store};

/// Whether the tab links here. The header marks the tab `aria-disabled` until
/// this view exists.
pub const READY: bool = true;

pub const NOTHING_MERGED: &str = "Nothing merged yet.";
const DASH: &str = "\u{2013}";
const DEFAULT_PERIOD: Period = Period::Last12Months(Grain::Week);

#[derive(Clone, PartialEq)]
enum Status {
    Loading,
    Ready(Box<StatsData>),
    Failed,
}

thread_local! {
    /// The last answer for each period, so a second visit paints at once.
    static ANSWERS: RefCell<Vec<(Period, StatsData)>> = const { RefCell::new(Vec::new()) };
}

fn remembered(period: Period) -> Option<StatsData> {
    ANSWERS.with_borrow(|all| {
        all.iter()
            .find(|(p, _)| *p == period)
            .map(|(_, stats)| stats.clone())
    })
}

fn remember(period: Period, stats: &StatsData) {
    ANSWERS.with_borrow_mut(|all| {
        all.retain(|(p, _)| *p != period);
        all.push((period, stats.clone()));
    });
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

/// "31 merged in 12 months across 9 repos, 74 percent merge rate".
pub fn summary_line(stats: &StatsData) -> String {
    if let Some(yoy) = stats
        .yoy
        .as_ref()
        .filter(|_| stats.period == Period::YearOverYear)
    {
        let (this_year, last_year) = yoy.months.iter().fold((0, 0), |(a, b), m| {
            (a + m.this_year.merged, b + m.last_year.merged)
        });
        if this_year + last_year == 0 {
            return NOTHING_MERGED.to_owned();
        }
        return format!(
            "{this_year} merged this year against {last_year} last year{}",
            rate_clause(stats.lands.summary.merge_rate)
        );
    }
    let output = &stats.output;
    if output.merged_total == 0 {
        return NOTHING_MERGED.to_owned();
    }
    let repos = if output.repos_total == 1 {
        "repo"
    } else {
        "repos"
    };
    format!(
        "{} merged in 12 months across {} {repos}{}",
        output.merged_total,
        output.repos_total,
        rate_clause(stats.lands.summary.merge_rate)
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

fn best_text(best: Option<Best>, month: bool) -> Option<String> {
    best.map(|b| {
        let when = if month {
            month_label(b.start)
        } else {
            short_date(b.start)
        };
        format!("{when}, {} merged", b.merged)
    })
}

fn size_label(size: SizeClass) -> &'static str {
    match size {
        SizeClass::Under50 => "Under 50 lines",
        SizeClass::From50To199 => "50 to 199 lines",
        SizeClass::From200To499 => "200 to 499 lines",
        SizeClass::From500Up => "500 or more",
    }
}

fn milestone_text(kind: MilestoneKind, repo: Option<&str>) -> String {
    let repo = repo.unwrap_or("a repo");
    match kind {
        MilestoneKind::FirstMergeInRepo => format!("First merge in {repo}"),
        MilestoneKind::Merged50 => "50th merged pull request".to_owned(),
        MilestoneKind::Merged100 => "100th merged pull request".to_owned(),
        MilestoneKind::FirstStarMerge => format!("First merge into {repo}, 1,000 or more stars"),
    }
}

fn load(period: Period, current: RwSignal<Period>, status: RwSignal<Status>, store: Store) {
    match remembered(period) {
        Some(hit) => status.set(Status::Ready(Box::new(hit))),
        None => status.set(Status::Loading),
    }
    spawn_local(async move {
        let answer = api::stats(period).await;
        if current.get_untracked() != period {
            return;
        }
        match answer {
            Ok(fresh) => {
                remember(period, &fresh);
                let same = matches!(status.get_untracked(), Status::Ready(old) if *old == fresh);
                if !same {
                    status.set(Status::Ready(Box::new(fresh)));
                }
            }
            Err(ApiError::Unauthorized) => store.refresh(),
            Err(_) => {
                if !matches!(status.get_untracked(), Status::Ready(_)) {
                    status.set(Status::Failed);
                }
            }
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

fn yoy_series(yoy: &Yoy, pick: fn(&domain::stats::MonthStats) -> Option<f64>) -> Vec<Series> {
    let months = |last: bool| {
        yoy.months
            .iter()
            .map(|m| pick(if last { &m.last_year } else { &m.this_year }))
            .collect()
    };
    vec![
        Series {
            name: "This year",
            tone: Tone::Text,
            values: months(false),
        },
        Series {
            name: "Last year",
            tone: Tone::Faint,
            values: months(true),
        },
    ]
}

fn yoy_labels(yoy: &Yoy) -> Vec<String> {
    yoy.months
        .iter()
        .map(|m| month_name(i64::from(m.month)).to_owned())
        .collect()
}

fn lands(lands: &Lands, yoy: Option<&Yoy>) -> impl IntoView {
    let s = &lands.summary;
    let ci_gap = yoy.is_some_and(|y| y.gaps.ci_failures);
    let cohort = lands.months.iter();
    let cohort_labels: Vec<String> = cohort.clone().map(|m| month_label(m.start)).collect();
    let opened: u32 = cohort.clone().map(|m| m.opened).sum();
    let merged: u32 = cohort.clone().map(|m| m.merged_within_30_days).sum();
    let cohort_series = vec![Series {
        name: "Merged within 30 days",
        tone: Tone::Text,
        values: cohort.map(|m| m.share).collect(),
    }];
    let compare = yoy.map(|y| {
        let label = "Merge rate by month, this year against last year";
        view! {
            {chart_title("Merge rate by month")}
            <Lines labels=yoy_labels(y) series=yoy_series(y, |m| m.merge_rate) scale=Scale::Percent label=label/>
        }
    });
    section(
        "Lands",
        view! {
            <dl class="stats-list">
                {stat("Merge rate", percent(s.merge_rate), "No PRs closed yet")}
                {stat("Median to merge", duration(s.median_hours_to_merge), "No PRs merged yet")}
                {stat("75th percentile", duration(s.p75_hours_to_merge), "No PRs merged yet")}
                {count_stat("Open over 30 days", lands.open_over_30_days)}
                {stat("Changes requested", percent(s.changes_requested_share), "No PRs yet")}
                {stat("CI failed at least once", percent(lands.ci_failed_share), "No CI results yet")}
            </dl>
            {ci_gap.then(|| note("Last year has no CI results, because the import carries none."))}
            {compare}
            {chart_title("Merged within 30 days, by month opened")}
            <Lines
                labels=cohort_labels
                series=cohort_series
                scale=Scale::Percent
                label=format!("Share of pull requests merged within 30 days, by month opened: {merged} of {opened}")
            />
        },
    )
}

fn reviews(r: &Reviews, period: Period, yoy: Option<&Yoy>) -> impl IntoView {
    let month = !weekly(period);
    let labels: Vec<String> = r
        .buckets
        .iter()
        .map(|b| {
            if month {
                month_label(b.start)
            } else {
                short_date(b.start)
            }
        })
        .collect();
    let series = |name, tone, pick: fn(&domain::stats::ReviewBucket) -> u32| Series {
        name,
        tone,
        values: r.buckets.iter().map(|b| Some(f64::from(pick(b)))).collect(),
    };
    let split = vec![
        series("Approved", Tone::Text, |b| b.approved),
        series("Changes requested", Tone::Muted, |b| b.changes_requested),
        series("Commented", Tone::Faint, |b| b.commented),
    ];
    let label = format!(
        "Reviews per {}: {} given, {} approved, {} changes requested, {} commented",
        if month { "month" } else { "week" },
        r.total,
        r.approved,
        r.changes_requested,
        r.commented
    );
    let repos: Vec<Row> = r
        .top_repos
        .iter()
        .map(|c| Row {
            label: c.repo.to_string(),
            value: f64::from(c.count),
            text: c.count.to_string(),
        })
        .collect();
    let max = peak(&repos);
    let compare = yoy.map(|y| {
        let label = "Reviews by month, this year against last year";
        view! {
            {chart_title("Reviews by month")}
            <Lines labels=yoy_labels(y) series=yoy_series(y, |m| Some(f64::from(m.reviews))) scale=Scale::Count label=label/>
        }
    });
    let top_label = format!(
        "Repos I review in, {} reviews across {} repos",
        r.total, r.repos_reviewed
    );
    section(
        "Reviews",
        view! {
            <dl class="stats-list">
                {count_stat("Reviews given", r.total)}
                {count_stat("Repos reviewed", r.repos_reviewed)}
                {count_stat("In other people's repos", r.external)}
                {count_stat("In my own repos", r.own)}
            </dl>
            {compare}
            {chart_title("Reviews given")}
            <Lines labels=labels series=split scale=Scale::Count label=label/>
            {if repos.is_empty() {
                stat("Repos I review in", None, "No reviews yet").into_any()
            } else {
                view! {
                    {chart_title("Repos I review in")}
                    <HBars rows=repos max=max label=top_label/>
                }
                    .into_any()
            }}
        },
    )
}

fn trust(t: &Trust) -> impl IntoView {
    let ranking: Vec<Row> = t
        .ranking
        .iter()
        .map(|r| Row {
            label: r.repo.to_string(),
            value: f64::from(r.merged),
            text: format!(
                "{} \u{b7} {} \u{b7} {}",
                r.merged,
                percent(r.merge_rate).unwrap_or_else(|| DASH.to_owned()),
                duration(r.median_first_response_hours).unwrap_or_else(|| DASH.to_owned())
            ),
        })
        .collect();
    let languages: Vec<Row> = t
        .languages
        .iter()
        .map(|l| Row {
            label: l.language.clone(),
            value: f64::from(l.merged),
            text: l.merged.to_string(),
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
                {count_stat("New repos", t.new_repos)}
                {count_stat("Repeat repos", t.repeat_repos)}
                {count_stat("Merges into 1,000+ stars", t.star_merges)}
            </dl>
            {if ranking.is_empty() {
                stat("Repo ranking", None, "No merged PRs yet").into_any()
            } else {
                view! {
                    {chart_title("Repos: merged, merge rate, first response")}
                    <HBars rows=ranking max=rank_max label=rank_label/>
                }
                    .into_any()
            }}
            {if languages.is_empty() {
                stat("Languages", None, "No merged PRs with a language yet").into_any()
            } else {
                view! {
                    {chart_title("Languages")}
                    <HBars rows=languages max=lang_max label=lang_label/>
                }
                    .into_any()
            }}
        },
    )
}

fn agents(agent: &Summary, other: &Summary) -> impl IntoView {
    let both = |pick: fn(&Summary) -> Option<String>| (pick(agent), pick(other));
    let cell = |value: Option<String>| value.unwrap_or_else(|| DASH.to_owned());
    let rows = [
        ("Pull requests", both(|s| Some(s.prs.to_string()))),
        ("Merge rate", both(|s| percent(s.merge_rate))),
        (
            "Median to merge",
            both(|s| duration(s.median_hours_to_merge)),
        ),
        (
            "Changes requested",
            both(|s| percent(s.changes_requested_share)),
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
            value: s.merge_rate.unwrap_or(0.0),
            text: percent(s.merge_rate).unwrap_or_else(|| DASH.to_owned()),
        })
        .collect::<Vec<_>>();
    let label = format!(
        "Merge rate, agent branches {}, other branches {}",
        percent(agent.merge_rate).unwrap_or_else(|| "none yet".to_owned()),
        percent(other.merge_rate).unwrap_or_else(|| "none yet".to_owned())
    );
    section(
        "Agents",
        view! {
            {if agent.prs == 0 {
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
                    <HBars rows=rates max=1.0 label=label/>
                }
                    .into_any()
            }}
        },
    )
}

fn output(o: &Output, period: Period, yoy: Option<&Yoy>) -> impl IntoView {
    let month = !weekly(period);
    let pairs: Vec<Pair> = if month {
        o.months
            .iter()
            .map(|m| Pair {
                start: m.start,
                opened: m.opened,
                merged: m.merged,
            })
            .collect()
    } else {
        o.weeks
            .iter()
            .map(|w| Pair {
                start: w.start,
                opened: w.opened,
                merged: w.merged,
            })
            .collect()
    };
    let average = o
        .weeks
        .last()
        .map(|w| format!("{:.1} a week", w.rolling_merged));
    let heat_total: u32 = o.heatmap.iter().map(|c| c.count).sum();
    let chart = match yoy {
        Some(y) => view! {
            {chart_title("Merged by month")}
            <Lines
                labels=yoy_labels(y)
                series=yoy_series(y, |m| Some(f64::from(m.merged)))
                scale=Scale::Count
                label="Pull requests merged by month, this year against last year"
            />
        }
        .into_any(),
        None => view! {
            {chart_title(if month { "Opened and merged by month" } else { "Opened and merged by week" })}
            <PairBars weeks=pairs unit=if month { "month" } else { "week" }/>
        }
        .into_any(),
    };
    section(
        "Output",
        view! {
            <dl class="stats-list">
                {stat("Best week", best_text(o.best_week, false), "No merges yet")}
                {stat("Best month", best_text(o.best_month, true), "No merges yet")}
                {stat("4 week average", average, "No weeks yet")}
                {count_stat("Streak of weeks", o.current_streak)}
                {count_stat("Longest streak", o.longest_streak)}
                {count_stat("Repos in 52 weeks", o.repos_total)}
            </dl>
            {chart}
            {chart_title("Activity per day")}
            <HeatMap cells=o.heatmap.clone()/>
            <p class="sr-only">{format!("{heat_total} events in the heatmap")}</p>
        },
    )
}

fn waiting(w: &Waiting, yoy: Option<&Yoy>) -> impl IntoView {
    let rows: Vec<Row> = w
        .stages
        .iter()
        .map(|s| Row {
            label: stage_label(s.stage).to_owned(),
            value: s.median_hours.unwrap_or(0.0),
            text: duration(s.median_hours).unwrap_or_else(|| DASH.to_owned()),
        })
        .collect();
    let any = w.stages.iter().any(|s| s.median_hours.is_some());
    let max = peak(&rows);
    let label = format!(
        "Median time in each stage, {} pull requests with stage history",
        w.prs
    );
    let gap = yoy.is_some_and(|y| y.gaps.stage_times || y.gaps.turn_times);
    section(
        "Waiting",
        view! {
            <dl class="stats-list">
                {stat("Waiting on maintainers", (w.prs > 0).then(|| days(w.days_waiting_on_them)), "No history yet")}
                {stat("Waiting on me", (w.prs > 0).then(|| days(w.days_waiting_on_me)), "No history yet")}
            </dl>
            {if any {
                view! {
                    {chart_title("Median time in each stage")}
                    <HBars rows=rows max=max label=label/>
                }
                    .into_any()
            } else {
                stat("Stage times", None, "History starts the day it ships").into_any()
            }}
            {gap.then(|| note("Last year has no stage or turn history."))}
        },
    )
}

fn shape(s: &Shape) -> impl IntoView {
    let rows: Vec<Row> = s
        .buckets
        .iter()
        .map(|b| Row {
            label: size_label(b.size).to_owned(),
            value: b.summary.merge_rate.unwrap_or(0.0),
            text: match (percent(b.summary.merge_rate), b.summary.prs) {
                (Some(rate), n) => format!("{rate} \u{b7} {n}"),
                (None, 0) => DASH.to_owned(),
                (None, n) => format!("{DASH} \u{b7} {n}"),
            },
        })
        .collect();
    let any = s.buckets.iter().any(|b| b.summary.merge_rate.is_some());
    let label = format!("Merge rate by size, {} size groups", s.buckets.len());
    section(
        "Shape",
        view! {
            <dl class="stats-list">{count_stat("Issues closed by my PRs", s.issues_closed)}</dl>
            {if any {
                view! {
                    {chart_title("Merge rate and PRs, by lines changed")}
                    <HBars rows=rows max=1.0 label=label/>
                }
                    .into_any()
            } else {
                stat("Merge rate by size", None, "No PRs closed yet").into_any()
            }}
        },
    )
}

fn habits(h: &Habits, yoy: Option<&Yoy>) -> impl IntoView {
    let goal = h.goal.map(|g| {
        let rows = vec![Row {
            label: "Merged this month".to_owned(),
            value: g.progress.min(1.0),
            text: format!("{} of {}", g.merged_this_month, g.target),
        }];
        let label = format!(
            "Progress toward the monthly goal, {} of {} merged",
            g.merged_this_month, g.target
        );
        view! {
            {chart_title("Monthly goal")}
            <HBars rows=rows max=1.0 label=label/>
        }
    });
    let milestones = h
        .milestones
        .iter()
        .map(|m| {
            let text = milestone_text(m.kind, m.repo.as_ref().map(|r| r.to_string()).as_deref());
            view! {
                <li>
                    <span>{text}</span>
                    <span class="stats-when">{short_date(m.at)}</span>
                </li>
            }
        })
        .collect_view();
    let none = h.milestones.is_empty();
    let gap = yoy.is_some_and(|y| y.gaps.habits);
    section(
        "Habits",
        view! {
            {if h.events == 0 {
                stat("When I work", None, "No activity of mine yet").into_any()
            } else {
                view! {
                    {chart_title("When I work")}
                    <Grid grid=h.grid.clone() label=format!("Activity by weekday and hour, {} events", h.events)/>
                }
                    .into_any()
            }}
            {gap.then(|| note("Last year has no activity history."))}
            {goal}
            {chart_title("Milestones")}
            {if none {
                stat("Milestones", None, "None reached in this period").into_any()
            } else {
                view! { <ul class="stats-milestones">{milestones}</ul> }.into_any()
            }}
        },
    )
}

fn body(stats: &StatsData) -> impl IntoView {
    let yoy = stats.yoy.as_ref();
    view! {
        <h1 class="stats-summary">{summary_line(stats)}</h1>
        <div class="stats-grid">
            {lands(&stats.lands, yoy)}
            {reviews(&stats.reviews, stats.period, yoy)}
            {trust(&stats.trust)}
            {agents(&stats.agents.agent, &stats.agents.other)}
            {output(&stats.output, stats.period, yoy)}
            {waiting(&stats.waiting, yoy)}
            {shape(&stats.shape)}
            {habits(&stats.habits, yoy)}
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
    let location = use_location();
    let now = store.now_secs();
    let period = RwSignal::new(address_period());
    let status = RwSignal::new(Status::Loading);
    let yoy_ready = RwSignal::new(None::<bool>);
    Effect::new(move |_| load(period.get(), period, status, store));
    Effect::new(move |_| {
        if let Status::Ready(stats) = status.get() {
            yoy_ready.set(Some(stats.yoy_ready));
        }
    });
    let choose = move |next: Period| {
        period.set(next);
        location.replace(&href(next));
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
                        let year = Timestamp(now.get()).year_month().0 + 1;
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
                Status::Ready(stats) => body(&stats).into_any(),
            }}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use domain::stats::{build_stats, ImportState, StatsInput};
    use domain::trends::build_trends;

    use super::*;

    const NOW: i64 = 1_794_830_400;

    fn empty(period: Period) -> StatsData {
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
        build_stats(&input, period)
    }

    #[test]
    fn the_summary_says_nothing_merged_for_zero() {
        assert_eq!(summary_line(&empty(DEFAULT_PERIOD)), "Nothing merged yet.");
        assert_eq!(
            summary_line(&empty(Period::YearOverYear)),
            "Nothing merged yet."
        );
    }

    #[test]
    fn the_summary_follows_one_and_many_and_drops_a_null_rate() {
        let mut stats = empty(DEFAULT_PERIOD);
        stats.output.merged_total = 31;
        stats.output.repos_total = 9;
        stats.lands.summary.merge_rate = Some(0.74);
        assert_eq!(
            summary_line(&stats),
            "31 merged in 12 months across 9 repos, 74 percent merge rate"
        );
        stats.output.merged_total = 1;
        stats.output.repos_total = 1;
        stats.lands.summary.merge_rate = None;
        assert_eq!(summary_line(&stats), "1 merged in 12 months across 1 repo");
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
    fn an_empty_answer_has_no_stray_words() {
        let stats = empty(DEFAULT_PERIOD);
        assert_eq!(stats.lands.summary.merge_rate, None);
        assert_eq!(percent(stats.lands.summary.merge_rate), None);
        assert!(stats.reviews.top_repos.is_empty());
        assert!(stats.habits.milestones.is_empty());
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
    fn size_and_milestone_labels_are_plain() {
        assert_eq!(size_label(SizeClass::From500Up), "500 or more");
        assert_eq!(
            milestone_text(MilestoneKind::FirstMergeInRepo, Some("acme/alpha")),
            "First merge in acme/alpha"
        );
        assert_eq!(
            milestone_text(MilestoneKind::Merged50, None),
            "50th merged pull request"
        );
    }

    #[test]
    fn the_last_answer_per_period_is_kept() {
        let stats = empty(DEFAULT_PERIOD);
        assert!(remembered(DEFAULT_PERIOD).is_none());
        remember(DEFAULT_PERIOD, &stats);
        remember(DEFAULT_PERIOD, &stats);
        assert_eq!(remembered(DEFAULT_PERIOD), Some(stats));
        assert!(remembered(Period::YearOverYear).is_none());
        assert_eq!(month_label(Timestamp(NOW)), "Nov");
    }
}
