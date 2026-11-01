//! The Wrapped cards, one component each, all on the kit's `StoryCard`.
//! The top lists (artists, songs, albums, genres, new artists) share
//! `TopListCard`.

use super::copy;
use crate::api::listening::{Ranked, WrappedYear};
use crate::api::{Capabilities, ServerConfig};
use crate::components::song_details::share_element_image;
use crate::components::ui::{cover_art_src, Artwork, PreviewRing, Skeleton, StoryCard};
use crate::components::{quick_listen_id, Icon, IsPlayingSignal, Preview, VolumeSignal};
use chrono::Datelike;
use dioxus::prelude::*;
use std::rc::Rc;

/// The element the Share button captures.
pub(super) const SUMMARY_ID: &str = "rs-wrapped-summary";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CardKind {
    Empty,
    Minutes,
    TopArtist,
    TopArtists,
    TopSong,
    TopSongs,
    TopAlbums,
    TopGenres,
    Clock,
    Streak,
    NewArtists,
    Personality,
    Summary,
}

/// The cards a year has something to say on, in story order.
pub(super) fn cards_for(year: &WrappedYear) -> Vec<CardKind> {
    use CardKind::*;
    if year.is_empty() {
        return vec![Empty];
    }
    let mut cards = vec![Minutes];
    let mut add = |kind, show: bool| {
        if show {
            cards.push(kind);
        }
    };
    add(TopArtist, !year.top_artists.is_empty());
    add(TopArtists, year.top_artists.len() > 1);
    add(TopSong, !year.top_songs.is_empty());
    add(TopSongs, year.top_songs.len() > 1);
    add(TopAlbums, !year.top_albums.is_empty());
    add(TopGenres, !year.top_genres.is_empty());
    add(Clock, year.hours.iter().any(|h| *h > 0));
    add(Streak, year.streak.is_some_and(|s| s.days >= 2));
    add(NewArtists, year.new_artist_count > 0);
    add(Personality, true);
    add(Summary, true);
    cards
}

/// Renders one card. `active`: it is the card on show (not the one leaving).
pub(super) fn render(kind: CardKind, year: Rc<WrappedYear>, active: bool) -> Element {
    match kind {
        CardKind::Empty => rsx! { EmptyCard { year } },
        CardKind::Minutes => rsx! { MinutesCard { year } },
        CardKind::TopArtist => rsx! { TopArtistCard { year } },
        CardKind::TopArtists => {
            let rows = rows(&year.top_artists, false, &|a: &Ranked| copy::plays(a.plays));
            let wash = year.top_artists.get(1).cloned();
            rsx! {
                TopListCard {
                    kicker: "Top artists".to_string(),
                    title: format!("The artists you kept coming back to {}", copy::when(&year)),
                    rows,
                    wash,
                    testid: "wrapped-card-top-artists".to_string(),
                }
            }
        }
        CardKind::TopSong => rsx! { TopSongCard { year, active } },
        CardKind::TopSongs => {
            let rows = rows(&year.top_songs, true, &|s: &Ranked| copy::plays(s.plays));
            let wash = year.top_songs.get(1).cloned();
            let title = if year.so_far { "Top songs played this year".to_string() } else { format!("Top songs last played in {}", year.year) };
            rsx! {
                TopListCard {
                    kicker: "Top songs".to_string(),
                    title,
                    rows,
                    wash,
                    note: copy::RANK_NOTE.to_string(),
                    testid: "wrapped-card-top-songs".to_string(),
                }
            }
        }
        CardKind::TopAlbums => {
            let rows = rows(&year.top_albums, true, &|a: &Ranked| copy::plays(a.plays));
            let wash = year.top_albums.first().cloned();
            rsx! {
                TopListCard {
                    kicker: "Top albums".to_string(),
                    title: format!("The records you lived with {}", copy::when(&year)),
                    rows,
                    wash,
                    testid: "wrapped-card-top-albums".to_string(),
                }
            }
        }
        CardKind::TopGenres => {
            let total = year.plays.max(1) as f64;
            let top = year.top_genres.first().map_or(1, |g| g.plays.max(1)) as f64;
            let rows = year
                .top_genres
                .iter()
                .map(|g| TopRow {
                    name: g.name.clone(),
                    detail: format!("{}%", ((g.plays as f64 / total) * 100.0).round().max(1.0)),
                    bar: Some(g.plays as f64 / top),
                    ..TopRow::default()
                })
                .collect();
            rsx! {
                TopListCard {
                    kicker: "Top genres".to_string(),
                    title: format!("What {} sounded like", if year.so_far { "this year".to_string() } else { year.year.to_string() }),
                    rows,
                    testid: "wrapped-card-top-genres".to_string(),
                }
            }
        }
        CardKind::Clock => rsx! { ClockCard { year } },
        CardKind::Streak => rsx! { StreakCard { year } },
        CardKind::NewArtists => {
            let rows = rows(&year.new_artists, false, &|a: &Ranked| copy::plays(a.plays));
            let n = year.new_artist_count;
            let title = if n == 1 { "1 artist new to you".to_string() } else { format!("{} artists new to you", copy::count(n as u64)) };
            rsx! {
                TopListCard {
                    kicker: "New to your library".to_string(),
                    title,
                    rows,
                    wash: year.new_artists.first().cloned(),
                    note: format!("Artists whose music joined your library {}.", copy::when(&year)),
                    testid: "wrapped-card-new-artists".to_string(),
                }
            }
        }
        CardKind::Personality => rsx! { PersonalityCard { year } },
        CardKind::Summary => rsx! { SummaryCard { year } },
    }
}

fn art(servers: &[ServerConfig], item: &Ranked, px: u32) -> Option<String> {
    cover_art_src(servers, &item.server_id, item.cover_art.as_deref(), px)
}

/// The album-art wash behind a card, from one item's cover.
fn use_wash(item: Option<&Ranked>) -> Option<String> {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    item.and_then(|i| art(&servers.read(), i, 120))
}

/// One row of a top list.
#[derive(Clone, PartialEq, Default)]
pub(super) struct TopRow {
    pub name: String,
    pub sub: Option<String>,
    pub detail: String,
    /// Artwork (cover id, server); none for genres.
    pub art: Option<(Option<String>, String)>,
    /// A share bar from 0 to 1, for genres.
    pub bar: Option<f64>,
}

fn rows(items: &[Ranked], with_by: bool, detail: &dyn Fn(&Ranked) -> String) -> Vec<TopRow> {
    items
        .iter()
        .map(|item| TopRow {
            name: item.name.clone(),
            sub: if with_by { item.by.clone() } else { None },
            detail: detail(item),
            art: Some((item.cover_art.clone(), item.server_id.clone())),
            bar: None,
        })
        .collect()
}

/// A numbered top-five list.
#[component]
pub(super) fn TopListCard(
    kicker: String,
    title: String,
    rows: Vec<TopRow>,
    #[props(default)] wash: Option<Ranked>,
    #[props(default)] note: Option<String>,
    testid: String,
) -> Element {
    let wash = use_wash(wash.as_ref());
    rsx! {
        StoryCard { wash, kicker, testid,
            h2 { class: "rs-wrapped-title", "{title}" }
            ol { class: "rs-wrapped-list",
                for (i, row) in rows.iter().enumerate() {
                    li { key: "{i}", class: "rs-wrapped-list__row",
                        span { class: "rs-wrapped-list__rank", "{i + 1}" }
                        if let Some((cover, server)) = row.art.clone() {
                            Artwork {
                                cover_art: cover,
                                server_id: server,
                                size_px: 48,
                                class: "rs-wrapped-list__art".to_string(),
                                eager: true,
                            }
                        }
                        span { class: "rs-wrapped-list__meta",
                            span { class: "rs-wrapped-list__name", "{row.name}" }
                            if let Some(sub) = row.sub.clone() {
                                span { class: "rs-wrapped-list__sub", "{sub}" }
                            }
                            if let Some(bar) = row.bar {
                                span { class: "rs-wrapped-list__bar", aria_hidden: "true",
                                    span { style: "width:{(bar.clamp(0.02, 1.0) * 100.0).round()}%" }
                                }
                            }
                        }
                        span { class: "rs-wrapped-list__detail", "{row.detail}" }
                    }
                }
            }
            if let Some(note) = note {
                p { class: "rs-wrapped-note", "{note}" }
            }
        }
    }
}

#[component]
fn EmptyCard(year: Rc<WrappedYear>) -> Element {
    let when = copy::when(&year);
    rsx! {
        StoryCard { kicker: copy::title(year.year, year.so_far), testid: "wrapped-card-empty".to_string(),
            h2 { class: "rs-wrapped-title", "Nothing to look back on {when} yet" }
            p { class: "rs-wrapped-line",
                "Wrapped is made from what your servers remember about your plays. Play some music and come back; it fills in as you listen."
            }
        }
    }
}

/// The calm placeholder while a year loads.
#[component]
pub(super) fn LoadingCard() -> Element {
    rsx! {
        StoryCard { kicker: "Looking back".to_string(), testid: "wrapped-card-loading".to_string(),
            div { class: "rs-wrapped-skel", aria_busy: "true", aria_label: "Loading your year",
                Skeleton { width: "62%".to_string(), height: "18px".to_string() }
                Skeleton { width: "80%".to_string(), height: "84px".to_string() }
                Skeleton { width: "54%".to_string(), height: "18px".to_string() }
                Skeleton { width: "70%".to_string(), height: "14px".to_string() }
            }
        }
    }
}

#[component]
fn MinutesCard(year: Rc<WrappedYear>) -> Element {
    let wash = use_wash(year.top_artists.first());
    rsx! {
        StoryCard { wash, kicker: copy::title(year.year, year.so_far), testid: "wrapped-card-minutes".to_string(),
            p { class: "rs-wrapped-lead", "You spent about" }
            p { class: "rs-wrapped-big", "data-testid": "wrapped-minutes", "{copy::count(year.minutes)}" }
            p { class: "rs-wrapped-lead", "minutes with {copy::songs_of(&year)}." }
            if let Some(line) = copy::comparison(&year) {
                p { class: "rs-wrapped-line", "{line}" }
            }
            p { class: "rs-wrapped-note", "{copy::MINUTES_NOTE}" }
        }
    }
}

#[component]
fn TopArtistCard(year: Rc<WrappedYear>) -> Element {
    let wash = use_wash(year.top_artists.first());
    let Some(top) = year.top_artists.first().cloned() else {
        return rsx! {};
    };
    let unit = if year.top_songs.is_empty() { "albums" } else { "songs" };
    let items = if top.items == 1 { format!("1 {}", unit.trim_end_matches('s')) } else { format!("{} {unit}", top.items) };
    rsx! {
        StoryCard { wash, kicker: "Your top artist".to_string(), testid: "wrapped-card-top-artist".to_string(),
            div { class: "rs-wrapped-hero",
                Artwork {
                    cover_art: top.cover_art.clone(),
                    server_id: top.server_id.clone(),
                    size_px: 320,
                    fill: true,
                    eager: true,
                    placeholder: "artist".to_string(),
                    class: "rs-wrapped-hero__art".to_string(),
                }
            }
            h2 { class: "rs-wrapped-name", "{top.name}" }
            p { class: "rs-wrapped-line", "{copy::plays(top.plays)} across {items}, about {copy::count(top.minutes)} minutes." }
        }
    }
}

/// The top song, with a Quick Listen. It starts by itself when the card
/// shows, but only when nothing is playing and the volume is up; otherwise
/// it waits for a tap. Leaving the card stops it (main playback resumes).
#[component]
fn TopSongCard(year: Rc<WrappedYear>, active: bool) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let preview = use_context::<Preview>();
    let is_playing = use_context::<IsPlayingSignal>().0;
    let volume = use_context::<VolumeSignal>().0;
    let top = year.top_songs.first().cloned().unwrap_or_default();
    let song = top.song.clone();
    let id = song.as_ref().map(quick_listen_id);
    let can_preview = song
        .as_ref()
        .is_some_and(|s| Capabilities::for_source(&servers.read(), &s.server_id).quick_listen);

    let stop_mine = {
        let id = id.clone();
        move || {
            if id.is_some() && *preview.current.peek() == id {
                preview.stop(true);
            }
        }
    };
    use_effect({
        let song = song.clone();
        move || {
            let quiet = !*is_playing.peek() && *volume.peek() > 0.0 && preview.current.peek().is_none();
            if let (true, true, Some(song)) = (active, can_preview && quiet, song.clone()) {
                preview.quick_listen(song, &servers.peek());
            }
        }
    });
    use_drop({
        let stop_mine = stop_mine.clone();
        move || stop_mine()
    });
    use_effect(use_reactive!(|active| {
        if !active {
            stop_mine();
        }
    }));

    let previewing = id.is_some() && *preview.current.read() == id;
    let wash = use_wash(Some(&top));
    rsx! {
        StoryCard { wash, kicker: "Your top song".to_string(), testid: "wrapped-card-top-song".to_string(),
            div { class: "rs-wrapped-hero",
                Artwork {
                    cover_art: top.cover_art.clone(),
                    server_id: top.server_id.clone(),
                    size_px: 320,
                    fill: true,
                    eager: true,
                    class: "rs-wrapped-hero__art".to_string(),
                }
            }
            h2 { class: "rs-wrapped-name", "{top.name}" }
            if let Some(by) = top.by.clone() {
                p { class: "rs-wrapped-lead rs-wrapped-lead--soft", "{by}" }
            }
            p { class: "rs-wrapped-line", "{copy::plays(top.plays)} in all, about {copy::count(top.minutes)} minutes." }
            if can_preview {
                button {
                    r#type: "button",
                    class: "rs-wrapped-preview",
                    "data-testid": "wrapped-preview",
                    aria_pressed: if previewing { "true" } else { "false" },
                    onclick: {
                        let song = song.clone();
                        move |_| {
                            if let Some(song) = song.clone() {
                                preview.quick_listen(song, &servers.peek());
                            }
                        }
                    },
                    span { class: "rs-wrapped-preview__icon",
                        Icon { name: if previewing { "pause".to_string() } else { "play".to_string() }, class: "rs-ic rs-ic--sm".to_string() }
                        if previewing {
                            PreviewRing {}
                        }
                    }
                    if previewing { "Playing a preview" } else { "Play a preview" }
                }
            }
        }
    }
}

#[component]
fn ClockCard(year: Rc<WrappedYear>) -> Element {
    let peak = year.peak_part();
    let max = year.hours.iter().copied().max().unwrap_or(0).max(1);
    let busiest = year.hours.iter().enumerate().max_by_key(|(h, c)| (**c, std::cmp::Reverse(*h))).map(|(h, _)| h).unwrap_or(0);
    rsx! {
        StoryCard { kicker: "When you listened".to_string(), testid: "wrapped-card-clock".to_string(),
            if let Some(part) = peak {
                h2 { class: "rs-wrapped-title", "You're {copy::part(part).0}." }
                p { class: "rs-wrapped-line", "Most of your listening happened {copy::part(part).1}." }
            }
            div {
                class: "rs-wrapped-clock",
                role: "img",
                aria_label: "Plays by hour of the day, most around {copy::hour(busiest)}",
                div { class: "rs-wrapped-clock__bars",
                    for (h, n) in year.hours.iter().enumerate() {
                        span {
                            key: "{h}",
                            class: "rs-wrapped-clock__bar",
                            title: "{copy::hour(h)}: {n}",
                            style: "height:{((*n as f64 / max as f64) * 100.0).round()}%",
                        }
                    }
                }
                div { class: "rs-wrapped-clock__axis", aria_hidden: "true",
                    span { "12 am" }
                    span { "6 am" }
                    span { "noon" }
                    span { "6 pm" }
                }
            }
            if let Some(day) = year.busiest_day {
                p { class: "rs-wrapped-line",
                    "Your busiest day was {copy::weekday(day.date.weekday().num_days_from_monday())}, {copy::date(day.date)}."
                }
            }
            if let Some(day) = year.busiest_weekday {
                p { class: "rs-wrapped-line rs-wrapped-line--soft", "{copy::weekday(day)}s were busiest overall." }
            }
            p { class: "rs-wrapped-note", "{copy::LAST_PLAY_NOTE}" }
        }
    }
}

#[component]
fn StreakCard(year: Rc<WrappedYear>) -> Element {
    let Some(streak) = year.streak else {
        return rsx! {};
    };
    rsx! {
        StoryCard { kicker: "Your longest streak".to_string(), testid: "wrapped-card-streak".to_string(),
            p { class: "rs-wrapped-big", "{streak.days}" }
            p { class: "rs-wrapped-lead", "days in a row with music." }
            p { class: "rs-wrapped-line", "{copy::date_range(streak.start, streak.end)}." }
            p { class: "rs-wrapped-note", "At least this long. {copy::LAST_PLAY_NOTE}" }
        }
    }
}

#[component]
fn PersonalityCard(year: Rc<WrappedYear>) -> Element {
    let (title, line) = copy::personality(&year.personality);
    let wash = use_wash(year.top_albums.first().or(year.top_artists.first()));
    let genre = year.top_genres.first().map(|g| g.name.clone());
    rsx! {
        StoryCard { wash, kicker: "Your music personality".to_string(), testid: "wrapped-card-personality".to_string(),
            h2 { class: "rs-wrapped-name", "{title}" }
            p { class: "rs-wrapped-lead rs-wrapped-lead--soft", "{line}" }
            dl { class: "rs-wrapped-facts",
                div {
                    dt { "Artists" }
                    dd { "{copy::count(year.artist_count as u64)}" }
                }
                div {
                    dt { if year.top_songs.is_empty() { "Albums" } else { "Songs" } }
                    dd { "{copy::count(year.listens as u64)}" }
                }
                if let Some(genre) = genre {
                    div {
                        dt { "Top genre" }
                        dd { "{genre}" }
                    }
                }
            }
        }
    }
}

/// What a share attempt came to, in words (None: nothing to say).
fn share_note(status: &str) -> Option<String> {
    match status {
        "shared-image" => Some("Shared.".to_string()),
        "saved-image" => Some("Saved as an image.".to_string()),
        "cancelled" => None,
        _ => Some("Couldn't make the image on this device.".to_string()),
    }
}

/// The last card: everything on one shareable card, and Share.
#[component]
fn SummaryCard(year: Rc<WrappedYear>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let mut pending = use_signal(|| false);
    let mut note = use_signal(|| None::<String>);
    let top = year.top_artists.first().cloned();
    let wash = top.as_ref().and_then(|t| art(&servers.read(), t, 120));
    let hero = top.as_ref().and_then(|t| art(&servers.read(), t, 240));
    let (title, _) = copy::personality(&year.personality);
    let (list_label, list) = if year.top_songs.is_empty() {
        ("Top albums", year.top_albums.clone())
    } else {
        ("Top songs", year.top_songs.clone())
    };
    let file_name = format!("rustysound-{}-in-music.png", year.year);

    rsx! {
        StoryCard { kicker: "Your year, on one card".to_string(), testid: "wrapped-card-summary".to_string(),
            div { id: SUMMARY_ID, class: "rs-wrapped-summary", "data-testid": "wrapped-summary",
                if let Some(url) = wash {
                    img { class: "rs-wrapped-summary__wash", src: "{url}", alt: "" }
                }
                div { class: "rs-wrapped-summary__veil" }
                div { class: "rs-wrapped-summary__body",
                    p { class: "rs-wrapped-summary__brand", "My {copy::title(year.year, year.so_far)} in music" }
                    if let Some(top) = top.clone() {
                        div { class: "rs-wrapped-summary__hero",
                            if let Some(url) = hero {
                                img { class: "rs-wrapped-summary__art", src: "{url}", alt: "" }
                            }
                            div {
                                p { class: "rs-wrapped-summary__label", "Top artist" }
                                p { class: "rs-wrapped-summary__name", "{top.name}" }
                            }
                        }
                    }
                    div { class: "rs-wrapped-summary__cols",
                        div {
                            p { class: "rs-wrapped-summary__label", "Top artists" }
                            ol {
                                for (i, a) in year.top_artists.iter().enumerate() {
                                    li { key: "{i}", "{a.name}" }
                                }
                            }
                        }
                        div {
                            p { class: "rs-wrapped-summary__label", "{list_label}" }
                            ol {
                                for (i, s) in list.iter().enumerate() {
                                    li { key: "{i}", "{s.name}" }
                                }
                            }
                        }
                    }
                    div { class: "rs-wrapped-summary__stats",
                        div {
                            p { class: "rs-wrapped-summary__label", "Minutes" }
                            p { class: "rs-wrapped-summary__value", "{copy::count(year.minutes)}" }
                        }
                        if let Some(genre) = year.top_genres.first() {
                            div {
                                p { class: "rs-wrapped-summary__label", "Top genre" }
                                p { class: "rs-wrapped-summary__value", "{genre.name}" }
                            }
                        }
                    }
                    p { class: "rs-wrapped-summary__foot", "{title} · RustySound" }
                }
            }
            div { class: "rs-wrapped-share",
                button {
                    r#type: "button",
                    class: "rs-wrapped-preview",
                    disabled: pending(),
                    "data-testid": "wrapped-share",
                    onclick: move |_| {
                        if *pending.peek() {
                            return;
                        }
                        pending.set(true);
                        note.set(None);
                        let file_name = file_name.clone();
                        spawn(async move {
                            let status = share_element_image(SUMMARY_ID.to_string(), file_name, "share".to_string()).await;
                            pending.set(false);
                            note.set(share_note(&status));
                        });
                    },
                    span { class: "rs-wrapped-preview__icon",
                        Icon { name: "share".to_string(), class: "rs-ic rs-ic--sm".to_string() }
                    }
                    if pending() { "Making the image…" } else { "Share" }
                }
                p { class: "rs-wrapped-share__note", role: "status", "data-testid": "wrapped-share-note",
                    if let Some(text) = note() { "{text}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::listening::Personality;

    fn empty() -> WrappedYear {
        WrappedYear {
            year: 2026,
            so_far: true,
            listens: 0,
            plays: 0,
            minutes: 0,
            previous_minutes: None,
            artist_count: 0,
            top_artists: Vec::new(),
            top_songs: Vec::new(),
            top_albums: Vec::new(),
            top_genres: Vec::new(),
            genre_count: 0,
            hours: [0; 24],
            busiest_day: None,
            busiest_weekday: None,
            streak: None,
            new_artists: Vec::new(),
            new_artist_count: 0,
            personality: Personality::Steady,
        }
    }

    #[test]
    fn cards_follow_the_data() {
        assert_eq!(cards_for(&empty()), vec![CardKind::Empty]);
        let mut year = empty();
        year.listens = 3;
        year.top_albums = vec![Ranked::default()];
        year.top_artists = vec![Ranked::default()];
        // Albums only: no song cards, one artist: no artist list.
        assert_eq!(
            cards_for(&year),
            vec![CardKind::Minutes, CardKind::TopArtist, CardKind::TopAlbums, CardKind::Personality, CardKind::Summary]
        );
        year.top_songs = vec![Ranked::default(), Ranked::default()];
        year.hours[20] = 2;
        let cards = cards_for(&year);
        assert!(cards.contains(&CardKind::TopSong) && cards.contains(&CardKind::TopSongs) && cards.contains(&CardKind::Clock));
        assert!(!cards.contains(&CardKind::Streak) && !cards.contains(&CardKind::NewArtists));
    }

    #[test]
    fn share_results_in_words() {
        assert_eq!(share_note("saved-image").as_deref(), Some("Saved as an image."));
        assert_eq!(share_note("cancelled"), None);
        assert!(share_note("capture-failed").is_some());
    }
}
