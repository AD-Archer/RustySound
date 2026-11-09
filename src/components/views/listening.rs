//! Listening: your listening stats from Last.fm.
//!
//! Without an account it asks for one (username + the user's own API key,
//! the same two fields as Settings › Discovery, checked before saving). With
//! one: a stage header with the profile and its totals, any year in review
//! (joining year to now), top artists, albums and songs for a period, and
//! the latest scrobbles. Either way, "Play as a Story" opens Wrapped, built
//! from the servers' own play history.
//!
//! Data: `api::listening::lastfm` (cached there). Reached from the sidebar,
//! Library and Home (compact).

use super::settings::{test_lastfm_key, test_lastfm_user};
use super::stats::{ago, number};
use super::WrappedStory;
use crate::api::listening::current_year;
use crate::api::listening::lastfm::{self, Account, Period, Profile, Recent, Top, Tops, Year};
use crate::components::ui::{
    use_layout, Artwork, Button, ButtonVariant, EmptyState, InsetList, ListInset, ListRow, SectionHeader,
    Segmented, Skeleton,
};
use crate::components::{update_settings, AppView, Icon, Navigation};
use crate::db::AppSettings;
use chrono::{TimeZone, Utc};
use dioxus::prelude::*;

/// Rows in each top list.
const TOP_LIMIT: u32 = 10;
/// Rows in each of a year's lists.
const YEAR_LIMIT: usize = 5;
const RECENT_LIMIT: u32 = 15;

#[component]
pub fn ListeningView() -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let account = use_memo(move || Account::from_settings(&settings.read()));
    let mut story = use_signal(|| None::<Option<i32>>);

    rsx! {
        div { class: "rs-view rs-view--flush rs-listening", "data-testid": "listening",
            match account() {
                Some(account) => rsx! {
                    Connected { account, on_story: move |year| story.set(Some(year)) }
                },
                None => rsx! {
                    Connect { on_story: move |_| story.set(Some(None)) }
                },
            }
            if let Some(year) = story() {
                WrappedStory { start_year: year, on_close: move |_| story.set(None) }
            }
        }
    }
}

// ---- Not connected ----------------------------------------------------------

#[derive(Clone, PartialEq)]
enum Check {
    Idle,
    Running,
    Failed(String),
}

/// The connect prompt: username and API key, checked with Last.fm, then saved.
#[component]
fn Connect(on_story: EventHandler<()>) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    let mut user = use_signal(|| settings.peek().lastfm_username.clone());
    let mut key = use_signal(|| settings.peek().lastfm_api_key.clone());
    let mut check = use_signal(|| Check::Idle);
    let running = check() == Check::Running;

    let connect = move |evt: FormEvent| {
        evt.prevent_default();
        if *check.peek() == Check::Running {
            return;
        }
        check.set(Check::Running);
        let (name, api_key) = (user.peek().trim().to_string(), key.peek().trim().to_string());
        spawn(async move {
            let result = match test_lastfm_key(api_key.clone()).await {
                Ok(_) => test_lastfm_user(api_key.clone(), name.clone()).await,
                Err(error) => Err(error),
            };
            match result {
                Ok(_) => {
                    check.set(Check::Idle);
                    update_settings(settings, |s| {
                        s.lastfm_username = name;
                        s.lastfm_api_key = api_key;
                    });
                }
                Err(error) => check.set(Check::Failed(error)),
            }
        });
    };

    rsx! {
        div { class: "rs-stage rs-stage--tall rs-listening-hero rs-listening-hero--connect",
            div { class: "rs-listening-hero__inner",
                div { class: "rs-listening-hero__glyph", aria_hidden: "true",
                    Icon { name: "stats".to_string(), class: "rs-ic rs-ic--xl".to_string() }
                }
                h1 { class: "rs-large-title", "Listening" }
                p { class: "rs-listening-hero__lede",
                    "Connect Last.fm to see your scrobbles, your top artists, albums and songs, and a look back at any year."
                }
            }
        }
        div { class: "rs-listening-page",
            form { class: "rs-listening-connect", "data-testid": "listening-connect", onsubmit: connect,
                ul { class: "rs-list rs-list--text rs-settings-fields",
                    li {
                        div { class: "rs-settings-field",
                            label { r#for: "listening-user", "Username" }
                            input {
                                id: "listening-user",
                                autocomplete: "off",
                                autocapitalize: "off",
                                spellcheck: "false",
                                placeholder: "Your Last.fm name",
                                value: "{user}",
                                oninput: move |evt| user.set(evt.value()),
                            }
                        }
                    }
                    li {
                        div { class: "rs-settings-field",
                            label { r#for: "listening-key", "API Key" }
                            input {
                                id: "listening-key",
                                r#type: "password",
                                autocomplete: "off",
                                autocapitalize: "off",
                                spellcheck: "false",
                                placeholder: "32 letters and digits",
                                aria_describedby: "listening-key-help",
                                value: "{key}",
                                oninput: move |evt| key.set(evt.value()),
                            }
                        }
                    }
                }
                p { class: "rs-listening-connect__help", id: "listening-key-help",
                    "Get a free key at "
                    a { href: "https://www.last.fm/api/account/create", target: "_blank", rel: "noopener", "last.fm/api/account/create" }
                    " (any application name works) and paste the “API key”, not the shared secret."
                }
                if let Check::Failed(error) = check() {
                    p { class: "rs-listening-connect__error", role: "alert", "data-testid": "listening-connect-error", "{error}" }
                }
                Button {
                    label: if running { "Checking…".to_string() } else { "Connect Last.fm".to_string() },
                    variant: ButtonVariant::Filled,
                    submit: true,
                    block: true,
                    disabled: running || user().trim().is_empty() || key().trim().is_empty(),
                    testid: "listening-connect-submit".to_string(),
                }
            }

            SectionHeader { title: "From Your Server".to_string() }
            InsetList { inset: ListInset::Icon, label: "From your server".to_string(),
                ListRow {
                    title: "Play Your Year as a Story".to_string(),
                    subtitle: "Wrapped, from your server's play history".to_string(),
                    icon: "headphones".to_string(),
                    chevron: true,
                    testid: "listening-story".to_string(),
                    onclick: move |_| on_story.call(()),
                }
            }
        }
    }
}

// ---- Connected ---------------------------------------------------------------

#[component]
fn Connected(account: Account, on_story: EventHandler<Option<i32>>) -> Element {
    let account = use_signal(|| account.clone());
    let this_year = use_hook(current_year);
    let mut period = use_signal(|| Period::Month);
    let mut year = use_signal(|| this_year);

    let profile = use_resource(move || async move { lastfm::profile(&account()).await });
    let tops = use_resource(move || {
        let period = period();
        async move { lastfm::tops(&account(), period, TOP_LIMIT).await }
    });
    let review = use_resource(move || {
        let year = year();
        async move { lastfm::year(&account(), year, YEAR_LIMIT).await }
    });
    let recent = use_resource(move || async move { lastfm::recent(&account(), RECENT_LIMIT).await });

    let profile_now = profile.read().clone();
    let years = match &profile_now {
        Some(Ok(profile)) => profile.years(this_year),
        _ => vec![this_year],
    };
    let picked_period = Period::ALL.iter().position(|p| *p == period()).unwrap_or(1);

    rsx! {
        Hero { profile: profile_now.clone(), user: account.read().user.clone() }
        div { class: "rs-listening-page",
            if let Some(Err(error)) = profile_now {
                AccountError { error }
            }

            // ---- Year in review ----
            div { class: "rs-listening-head",
                h2 { class: "rs-hd__title", "Year in Review" }
                label { class: "rs-listening-year",
                    span { class: "rs-sr-only", "Year" }
                    select {
                        "data-testid": "listening-year",
                        value: "{year}",
                        onchange: move |evt| {
                            if let Ok(next) = evt.value().parse::<i32>() {
                                year.set(next);
                            }
                        },
                        for y in years {
                            option { key: "{y}", value: "{y}", selected: y == year(),
                                if y == this_year { "{y} so far" } else { "{y}" }
                            }
                        }
                    }
                    Icon { name: "chevron-down".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                }
            }
            match review.read().clone() {
                None => rsx! { Skeleton { rows: 4 } },
                Some(Err(error)) => rsx! { p { class: "rs-listening-note", role: "status", "{error}" } },
                Some(Ok(review)) => rsx! {
                    YearCard { review, so_far: year() == this_year, on_story: move |y| on_story.call(Some(y)) }
                },
            }

            // ---- Top ----
            div { class: "rs-listening-head",
                h2 { class: "rs-hd__title", "Top" }
            }
            div { class: "rs-listening-periods",
                Segmented {
                    options: Period::ALL.iter().map(|p| p.label().to_string()).collect::<Vec<_>>(),
                    selected: picked_period,
                    label: "Period".to_string(),
                    testid: "listening-period".to_string(),
                    on_select: move |index: usize| period.set(Period::ALL[index]),
                }
            }
            match tops.read().clone() {
                None => rsx! { Skeleton { rows: 6 } },
                Some(Err(error)) => rsx! { p { class: "rs-listening-note", role: "status", "{error}" } },
                Some(Ok(tops)) => rsx! { TopColumns { tops, testid: "listening-top".to_string() } },
            }

            // ---- Recent ----
            SectionHeader { title: "Recently Played".to_string() }
            match recent.read().clone() {
                None => rsx! { Skeleton { rows: 5 } },
                Some(Err(error)) => rsx! { p { class: "rs-listening-note", role: "status", "{error}" } },
                Some(Ok(tracks)) => rsx! { RecentList { tracks } },
            }

            AccountFooter { user: account.read().user.clone() }
        }
    }
}

/// The stage header: who, since when, and the lifetime totals.
#[component]
fn Hero(profile: Option<Result<Profile, String>>, user: String) -> Element {
    let profile = profile.and_then(Result::ok);
    let now = Utc::now().timestamp();
    let tiles: Vec<(&str, String)> = match &profile {
        Some(p) => vec![
            ("Scrobbles", number(Some(p.scrobbles))),
            ("Artists", number(Some(p.artists))),
            ("Albums", number(Some(p.albums))),
            ("Songs", number(Some(p.tracks))),
            ("Per Day", format!("{:.0}", p.per_day(now))),
            ("Loved", number(p.loved)),
        ],
        None => ["Scrobbles", "Artists", "Albums", "Songs", "Per Day", "Loved"]
            .into_iter()
            .map(|label| (label, "—".to_string()))
            .collect(),
    };
    let name = profile
        .as_ref()
        .map(|p| p.real_name.clone().unwrap_or_else(|| p.name.clone()))
        .unwrap_or(user.clone());
    let since = profile.as_ref().map(|p| {
        let mut line = format!("Scrobbling since {}", p.since_year());
        if let Some(country) = &p.country {
            line.push_str(&format!(" · {country}"));
        }
        line
    });
    let link = profile.as_ref().and_then(|p| p.url.clone());

    rsx! {
        div { class: "rs-stage rs-stage--tall rs-listening-hero", "data-testid": "listening-hero",
            div { class: "rs-listening-hero__inner",
                div { class: "rs-listening-who",
                    Artwork {
                        src: profile.as_ref().and_then(|p| p.image.clone()),
                        size_px: 72,
                        round: true,
                        eager: true,
                        placeholder: "artist".to_string(),
                    }
                    div { class: "rs-listening-who__text",
                        p { class: "rs-listening-who__kicker", "Listening" }
                        h1 { class: "rs-listening-who__name", "{name}" }
                        if let Some(since) = since {
                            p { class: "rs-listening-who__since", "{since}" }
                        }
                    }
                    if let Some(link) = link {
                        a {
                            class: "rs-btn rs-btn--tinted rs-btn--small rs-listening-who__link",
                            href: "{link}",
                            target: "_blank",
                            rel: "noopener",
                            "Last.fm"
                            Icon { name: "external".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                        }
                    }
                }
                dl { class: "rs-listening-tiles",
                    for (label, value) in tiles {
                        div { key: "{label}", class: "rs-listening-tile",
                            dt { "{label}" }
                            dd { "{value}" }
                        }
                    }
                }
            }
        }
    }
}

/// A year: its totals on a small stage, the top album covers, then the top
/// artists, albums and songs.
#[component]
fn YearCard(review: Year, so_far: bool, on_story: EventHandler<i32>) -> Element {
    if review.scrobbles == 0 && review.tops.artists.is_empty() {
        return rsx! {
            EmptyState {
                icon: "stats".to_string(),
                title: format!("Nothing scrobbled in {}", review.year),
                message: "Pick another year.".to_string(),
            }
        };
    }
    let top_artist = review.tops.artists.first().map(|a| a.name.clone());
    let year = review.year;
    let label = if so_far { format!("{year} so far") } else { year.to_string() };
    rsx! {
        section { class: "rs-stage rs-listening-year-card", aria_label: "{label} in review", "data-testid": "listening-year-card",
            div { class: "rs-listening-year-card__text",
                p { class: "rs-listening-year-card__kicker", "{label}" }
                p { class: "rs-listening-year-card__big",
                    span { class: "rs-listening-year-card__num", "{number(Some(review.scrobbles))}" }
                    " scrobbles"
                }
                p { class: "rs-listening-year-card__sub",
                    "{number(Some(review.artist_count))} artists · {number(Some(review.album_count))} albums · {number(Some(review.track_count))} songs"
                }
                if let Some(artist) = top_artist {
                    p { class: "rs-listening-year-card__top",
                        span { "Top artist" }
                        strong { "{artist}" }
                    }
                }
                button {
                    r#type: "button",
                    class: "rs-btn rs-btn--tinted rs-btn--small rs-listening-year-card__story",
                    "data-testid": "listening-story",
                    onclick: move |_| on_story.call(year),
                    Icon { name: "play".to_string(), class: "rs-ic rs-ic--xs".to_string() }
                    "Play as a Story"
                }
            }
            div { class: "rs-listening-year-card__covers", aria_hidden: "true",
                for (index, album) in review.tops.albums.iter().take(5).enumerate() {
                    div { key: "{index}", class: "rs-listening-year-card__cover", style: "--i:{index}",
                        Artwork {
                            src: album.image.clone(),
                            size_px: 120,
                            fill: true,
                            hero: true,
                            placeholder: "album".to_string(),
                        }
                    }
                }
            }
        }
        TopColumns { tops: review.tops, testid: "listening-year-tops".to_string() }
    }
}

/// Artists, albums and songs side by side (wide) or one after another.
#[component]
fn TopColumns(tops: Tops, testid: String) -> Element {
    rsx! {
        div { class: "rs-listening-columns", "data-testid": "{testid}",
            TopList { title: "Artists".to_string(), items: tops.artists, round: true, placeholder: "artist".to_string() }
            TopList { title: "Albums".to_string(), items: tops.albums, round: false, placeholder: "album".to_string() }
            TopList { title: "Songs".to_string(), items: tops.tracks, round: false, placeholder: "music".to_string() }
        }
    }
}

#[component]
fn TopList(title: String, items: Vec<Top>, round: bool, placeholder: String) -> Element {
    let most = items.iter().map(|item| item.plays).max().unwrap_or(1).max(1);
    rsx! {
        div { class: "rs-listening-col",
            h3 { class: "rs-listening-col__title", "{title}" }
            if items.is_empty() {
                p { class: "rs-listening-note", "Nothing yet." }
            } else {
                InsetList { inset: ListInset::Art, label: title.clone(),
                    for (index, item) in items.into_iter().enumerate() {
                        ListRow {
                            key: "{index}",
                            title: item.name.clone(),
                            compact: true,
                            detail: plays(item.plays),
                            leading: rsx! {
                                span { class: "rs-listening-rank", "{index + 1}" }
                                Artwork {
                                    src: item.image.clone(),
                                    size_px: 44,
                                    round,
                                    placeholder: placeholder.clone(),
                                }
                            },
                            sub_line: rsx! {
                                if let Some(artist) = item.artist.clone() {
                                    span { class: "rs-listening-sub", "{artist}" }
                                }
                                span { class: "rs-listening-bar", aria_hidden: "true",
                                    i { style: "transform:scaleX({item.plays as f64 / most as f64:.3})" }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

fn plays(count: u64) -> String {
    match count {
        1 => "1 play".to_string(),
        n => format!("{} plays", number(Some(n))),
    }
}

#[component]
fn RecentList(tracks: Vec<Recent>) -> Element {
    if tracks.is_empty() {
        return rsx! { p { class: "rs-listening-note", "No scrobbles yet." } };
    }
    rsx! {
        InsetList { inset: ListInset::Art, label: "Recently played".to_string(), testid: "listening-recent".to_string(),
            for (index, track) in tracks.into_iter().enumerate() {
                ListRow {
                    key: "{index}",
                    title: track.name.clone(),
                    subtitle: match &track.album {
                        Some(album) => format!("{} · {album}", track.artist),
                        None => track.artist.clone(),
                    },
                    compact: true,
                    current: track.now_playing,
                    detail: if track.now_playing {
                        "Now".to_string()
                    } else {
                        track
                            .played_at
                            .and_then(|t| Utc.timestamp_opt(t, 0).single())
                            .and_then(|t| ago(&t.to_rfc3339()))
                            .unwrap_or_default()
                    },
                    leading: rsx! {
                        Artwork { src: track.image.clone(), size_px: 44, placeholder: "music".to_string() }
                    },
                }
            }
        }
    }
}

#[component]
fn AccountError(error: String) -> Element {
    let navigation = use_context::<Navigation>();
    rsx! {
        div { class: "rs-listening-alert", role: "alert",
            p { "Couldn't reach Last.fm: {error}" }
            Button {
                label: "Check Settings".to_string(),
                variant: ButtonVariant::Plain,
                onclick: move |_| navigation.navigate_to(AppView::SettingsView {}),
            }
        }
    }
}

/// Who is connected, and a way out.
#[component]
fn AccountFooter(user: String) -> Element {
    let settings = use_context::<Signal<AppSettings>>();
    rsx! {
        SectionHeader { title: "Account".to_string() }
        InsetList { inset: ListInset::Icon, label: "Last.fm account".to_string(),
            ListRow { title: "Last.fm".to_string(), icon: "stats".to_string(), detail: user.clone(), compact: true }
            ListRow {
                title: "Disconnect".to_string(),
                icon: "x".to_string(),
                compact: true,
                testid: "listening-disconnect".to_string(),
                onclick: move |_| {
                    // The key stays (Discover uses it); without a name, this
                    // page asks again.
                    update_settings(settings, |s| s.lastfm_username.clear());
                },
            }
        }
        p { class: "rs-listening-foot", "Stats come from Last.fm and update about once an hour." }
    }
}

/// Home's way in on compact layouts (the sidebar has its own).
#[component]
pub(crate) fn ListeningEntry() -> Element {
    let navigation = use_context::<Navigation>();
    if !use_layout().is_compact() {
        return rsx! {};
    }
    rsx! {
        InsetList { inset: ListInset::Icon, label: "Listening".to_string(), class: "rs-listening-entry".to_string(),
            ListRow {
                title: "Listening Stats".to_string(),
                subtitle: "Your top artists, albums and years, from Last.fm".to_string(),
                icon: "stats".to_string(),
                chevron: true,
                testid: "home-listening".to_string(),
                onclick: move |_| navigation.navigate_to(AppView::ListeningView {}),
            }
        }
    }
}
