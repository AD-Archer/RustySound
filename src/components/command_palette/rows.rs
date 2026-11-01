//! The palette's list: one flat run of rows in sections, built from the
//! action list, the recents and the library's answer. Pure, so the order
//! rules are unit-tested.
//!
//! * Nothing typed: Recent, then every action by group.
//! * Typing: actions that clearly match ("nex" → Next) first, then the
//!   library's kinds, the best-matching kind first, then weaker actions.
//!   Among equal action matches the recently used come first.
//! * A choices page ("Sleep Timer…"): its choices, filtered by the query.

use super::actions::{Action, ActionId, Group};
use super::fuzzy::{fuzzy, STRONG};
use super::library::LibraryHits;
use super::recent::{action_rank, Recent};
use crate::api::{Album, Artist, Playlist, Song};
use crate::components::audio_manager::{SleepChoice, PLAYBACK_SPEEDS, SLEEP_CHOICES};
use crate::components::ui::env::{APPEARANCES, STAGE_THEMES};
use crate::components::speed_label;
use crate::equalizer::EqPreset;

/// Actions shown at most while typing.
const ACTION_CAP: usize = 8;
/// Recents shown before typing.
const RECENT_CAP: usize = 6;

/// A list of choices the palette opens in place ("Sleep Timer…").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChoicePage {
    Sleep,
    Speed,
    Equalizer,
    Theme,
    Profile,
}

impl ChoicePage {
    pub fn for_action(id: ActionId) -> Option<Self> {
        Some(match id {
            ActionId::SleepTimer => ChoicePage::Sleep,
            ActionId::PlaybackSpeed => ChoicePage::Speed,
            ActionId::Equalizer => ChoicePage::Equalizer,
            ActionId::Theme => ChoicePage::Theme,
            ActionId::Profile => ChoicePage::Profile,
            _ => return None,
        })
    }

    pub fn title(self) -> &'static str {
        match self {
            ChoicePage::Sleep => "Sleep Timer",
            ChoicePage::Speed => "Playback Speed",
            ChoicePage::Equalizer => "Equalizer",
            ChoicePage::Theme => "Theme",
            ChoicePage::Profile => "Profile",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Sleep(SleepChoice),
    Speed(f64),
    EqOff,
    Eq(EqPreset),
    Theme(&'static str),
    Appearance(&'static str),
    Profile(String),
}

/// What a row runs.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Action(ActionId),
    Choice(Choice),
    Album(Album),
    Artist(Artist),
    Song(Song),
    Playlist(Playlist),
}

impl Entry {
    /// The ⌘↩ action's name, when the row has one.
    pub fn alternate(&self) -> Option<&'static str> {
        match self {
            Entry::Song(_) | Entry::Album(_) | Entry::Playlist(_) => Some("Play Next"),
            _ => None,
        }
    }

    /// What ↩ does, for the footer.
    pub fn primary(&self) -> &'static str {
        match self {
            Entry::Action(_) | Entry::Choice(_) => "Run",
            Entry::Song(_) => "Play",
            _ => "Open",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Recent,
    Group(Group),
    Actions,
    Albums,
    Artists,
    Songs,
    Playlists,
    Choices,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Section::Recent => "Recent",
            Section::Group(group) => group.label(),
            Section::Actions => "Actions",
            Section::Albums => "Albums",
            Section::Artists => "Artists",
            Section::Songs => "Songs",
            Section::Playlists => "Playlists",
            Section::Choices => "Choices",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub entry: Entry,
    pub section: Section,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: &'static str,
    /// Matched chars of `title`, highlighted.
    pub hits: Vec<usize>,
    /// The current choice.
    pub checked: bool,
    /// Opens choices ("…" actions): drawn with a chevron.
    pub opens: bool,
}

fn action_row(action: &Action, section: Section, hits: Vec<usize>) -> Row {
    Row {
        entry: Entry::Action(action.id),
        section,
        title: action.title.clone(),
        subtitle: None,
        icon: action.icon,
        hits,
        checked: false,
        opens: action.choices,
    }
}

fn album_row(album: &Album, section: Section, query: &str) -> Row {
    Row {
        entry: Entry::Album(album.clone()),
        section,
        title: album.name.clone(),
        subtitle: Some(album.artist.clone()).filter(|a| !a.is_empty()),
        icon: "album",
        hits: fuzzy(query, &album.name).map(|m| m.hits).unwrap_or_default(),
        checked: false,
        opens: false,
    }
}

fn artist_row(artist: &Artist, section: Section, query: &str) -> Row {
    Row {
        entry: Entry::Artist(artist.clone()),
        section,
        title: artist.name.clone(),
        subtitle: Some("Artist".to_string()),
        icon: "artist",
        hits: fuzzy(query, &artist.name).map(|m| m.hits).unwrap_or_default(),
        checked: false,
        opens: false,
    }
}

fn song_row(song: &Song, section: Section, query: &str) -> Row {
    let subtitle = [song.artist.clone(), song.album.clone()]
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    Row {
        entry: Entry::Song(song.clone()),
        section,
        title: song.title.clone(),
        subtitle: Some(subtitle).filter(|s| !s.is_empty()),
        icon: "music",
        hits: fuzzy(query, &song.title).map(|m| m.hits).unwrap_or_default(),
        checked: false,
        opens: false,
    }
}

fn playlist_row(playlist: &Playlist, section: Section, query: &str) -> Row {
    Row {
        entry: Entry::Playlist(playlist.clone()),
        section,
        title: playlist.name.clone(),
        subtitle: Some("Playlist".to_string()),
        icon: "playlist",
        hits: fuzzy(query, &playlist.name).map(|m| m.hits).unwrap_or_default(),
        checked: false,
        opens: false,
    }
}

/// An action's match: its title (highlighted), else its keywords (weaker,
/// no highlight). Recently used actions get a small lift.
fn score_action(action: &Action, query: &str, recents: &[Recent]) -> Option<(i32, Vec<usize>)> {
    let (score, hits) = match fuzzy(query, &action.title) {
        Some(m) => (m.score, m.hits),
        None => {
            let m = fuzzy(query, action.keywords)?;
            ((m.score * 3 / 4).min(STRONG - 1), Vec::new())
        }
    };
    let lift = action_rank(recents, action.id.key()).map_or(0, |rank| 12 - rank as i32);
    Some((score + lift, hits))
}

/// The root list (no choices page open).
pub fn root_rows(query: &str, actions: &[Action], recents: &[Recent], library: Option<&LibraryHits>) -> Vec<Row> {
    let query = query.trim();
    let mut rows = Vec::new();

    if query.is_empty() {
        let mut shown = Vec::new();
        for recent in recents.iter().take(RECENT_CAP) {
            let row = match recent {
                Recent::Action { key } => {
                    let Some(action) = actions.iter().find(|a| a.id.key() == key) else {
                        continue;
                    };
                    shown.push(action.id);
                    action_row(action, Section::Recent, Vec::new())
                }
                Recent::Album { album } => album_row(album, Section::Recent, ""),
                Recent::Artist { artist } => artist_row(artist, Section::Recent, ""),
                Recent::Song { song } => song_row(song, Section::Recent, ""),
                Recent::Playlist { playlist } => playlist_row(playlist, Section::Recent, ""),
            };
            rows.push(row);
        }
        for group in [Group::Playback, Group::Song, Group::Pages, Group::App] {
            for action in actions.iter().filter(|a| a.group == group && !shown.contains(&a.id)) {
                rows.push(action_row(action, Section::Group(group), Vec::new()));
            }
        }
        return rows;
    }

    let mut scored: Vec<(i32, Row)> = actions
        .iter()
        .filter_map(|a| score_action(a, query, recents).map(|(s, hits)| (s, action_row(a, Section::Actions, hits))))
        .collect();
    // Stable: equal scores keep the list's order.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.truncate(ACTION_CAP);
    let best_action = scored.first().map_or(0, |(s, _)| *s);
    let action_rows: Vec<Row> = scored.into_iter().map(|(_, row)| row).collect();

    // Library kinds, the one whose leader matches best first (ties keep
    // Search's order: artists, albums, songs, then playlists).
    let mut kinds: Vec<(i32, Vec<Row>)> = Vec::new();
    if let Some(hits) = library {
        let lead = |title: &str| fuzzy(query, title).map_or(0, |m| m.score);
        let groups = [
            (hits.artists.first().map_or(0, |a| lead(&a.name)), hits.artists.iter().map(|a| artist_row(a, Section::Artists, query)).collect::<Vec<_>>()),
            (hits.albums.first().map_or(0, |a| lead(&a.name)), hits.albums.iter().map(|a| album_row(a, Section::Albums, query)).collect()),
            (hits.songs.first().map_or(0, |s| lead(&s.title)), hits.songs.iter().map(|s| song_row(s, Section::Songs, query)).collect()),
            (hits.playlists.first().map_or(0, |p| lead(&p.name)), hits.playlists.iter().map(|p| playlist_row(p, Section::Playlists, query)).collect()),
        ];
        kinds.extend(groups.into_iter().filter(|(_, rows)| !rows.is_empty()));
        kinds.sort_by(|a, b| b.0.cmp(&a.0));
    }
    let best_library = kinds.first().map_or(0, |(s, _)| *s);

    let actions_first = best_action >= STRONG && best_action >= best_library;
    if actions_first {
        rows.extend(action_rows.iter().cloned());
    }
    for (_, kind_rows) in kinds {
        rows.extend(kind_rows);
    }
    if !actions_first {
        rows.extend(action_rows);
    }
    rows
}

/// What the choices pages mark as current.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChoiceState {
    pub sleep: Option<SleepChoice>,
    pub speed: f64,
    pub eq_enabled: bool,
    pub eq_preset: EqPreset,
    pub theme: &'static str,
    pub appearance: &'static str,
    /// (id, name, active)
    pub profiles: Vec<(String, String, bool)>,
}

/// A choices page's rows, filtered by `query`.
pub fn choice_rows(page: ChoicePage, query: &str, state: &ChoiceState) -> Vec<Row> {
    let choice = |entry: Choice, title: String, icon: &'static str, checked: bool| Row {
        entry: Entry::Choice(entry),
        section: Section::Choices,
        title,
        subtitle: None,
        icon,
        hits: Vec::new(),
        checked,
        opens: false,
    };
    let all: Vec<Row> = match page {
        ChoicePage::Sleep => SLEEP_CHOICES
            .iter()
            .map(|c| choice(Choice::Sleep(*c), c.label(), "moon", state.sleep == Some(*c)))
            .collect(),
        ChoicePage::Speed => PLAYBACK_SPEEDS
            .iter()
            .map(|rate| choice(Choice::Speed(*rate), speed_label(*rate), "speed", (state.speed - rate).abs() < 0.01))
            .collect(),
        ChoicePage::Equalizer => std::iter::once(choice(Choice::EqOff, "Off".to_string(), "sliders", !state.eq_enabled))
            .chain(
                EqPreset::ALL
                    .iter()
                    .filter(|p| **p != EqPreset::Custom)
                    .map(|p| choice(Choice::Eq(*p), p.label().to_string(), "sliders", state.eq_enabled && state.eq_preset == *p)),
            )
            .collect(),
        ChoicePage::Theme => STAGE_THEMES
            .iter()
            .map(|(key, name, _)| choice(Choice::Theme(key), name.to_string(), "swatch", state.theme == *key))
            .chain(APPEARANCES.iter().map(|(key, name)| {
                choice(Choice::Appearance(key), format!("{name} Appearance"), "eye", state.appearance == *key)
            }))
            .collect(),
        ChoicePage::Profile => state
            .profiles
            .iter()
            .map(|(id, name, active)| choice(Choice::Profile(id.clone()), name.clone(), "headphones", *active))
            .collect(),
    };
    let query = query.trim();
    if query.is_empty() {
        return all;
    }
    let mut scored: Vec<(i32, Row)> = all
        .into_iter()
        .filter_map(|mut row| {
            let m = fuzzy(query, &row.title)?;
            row.hits = m.hits;
            Some((m.score, row))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, row)| row).collect()
}

/// Index of the first row of the section after (or before) `active`'s, for
/// Tab and Shift+Tab; wraps around.
pub fn next_section(rows: &[Row], active: usize, back: bool) -> usize {
    let mut starts: Vec<usize> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index == 0 || rows[index - 1].section != row.section {
            starts.push(index);
        }
    }
    if starts.is_empty() {
        return 0;
    }
    let current = starts.iter().rposition(|&s| s <= active).unwrap_or(0);
    let target = if back {
        // Back to this section's top first, then the previous section.
        if starts[current] < active {
            current
        } else {
            (current + starts.len() - 1) % starts.len()
        }
    } else {
        (current + 1) % starts.len()
    };
    starts[target]
}

#[cfg(test)]
mod tests {
    use super::super::actions::{actions, Caps, SongCaps};
    use super::*;

    fn playing() -> Vec<Action> {
        actions(&Caps {
            song: Some(SongCaps { album: true, artist: true, ..SongCaps::default() }),
            playing: true,
            ..Caps::default()
        })
    }

    fn sections(rows: &[Row]) -> Vec<Section> {
        let mut out: Vec<Section> = Vec::new();
        for row in rows {
            if out.last() != Some(&row.section) {
                out.push(row.section);
            }
        }
        out
    }

    #[test]
    fn empty_query_lists_recents_then_groups() {
        let recents = vec![
            Recent::Action { key: "lyrics".into() },
            Recent::Album { album: Album { id: "a".into(), name: "Paper Comets".into(), ..Default::default() } },
            Recent::Action { key: "gone".into() },
        ];
        let rows = root_rows("", &playing(), &recents, None);
        assert_eq!(rows[0].entry, Entry::Action(ActionId::Lyrics));
        assert!(matches!(rows[1].entry, Entry::Album(_)));
        assert_eq!(
            sections(&rows),
            vec![Section::Recent, Section::Group(Group::Playback), Section::Group(Group::Song), Section::Group(Group::Pages), Section::Group(Group::App)]
        );
        // Lyrics shows once, under Recent.
        assert_eq!(rows.iter().filter(|r| r.entry == Entry::Action(ActionId::Lyrics)).count(), 1);
    }

    #[test]
    fn a_clear_action_match_leads_and_a_library_match_leads_otherwise() {
        let library = LibraryHits {
            query: "next".into(),
            songs: vec![Song { id: "s".into(), title: "Nextdoor Blues".into(), ..Default::default() }],
            ..Default::default()
        };
        let rows = root_rows("next", &playing(), &[], Some(&library));
        assert_eq!(rows[0].entry, Entry::Action(ActionId::Next));
        assert_eq!(rows[0].hits, vec![0, 1, 2, 3]);

        let library = LibraryHits {
            query: "paper comets".into(),
            albums: vec![Album { id: "a".into(), name: "Paper Comets".into(), ..Default::default() }],
            songs: vec![Song { id: "s".into(), title: "Comets".into(), ..Default::default() }],
            ..Default::default()
        };
        let rows = root_rows("paper comets", &playing(), &[], Some(&library));
        assert!(matches!(rows[0].entry, Entry::Album(_)), "{:?}", rows[0]);
        assert_eq!(sections(&rows)[..2], [Section::Albums, Section::Songs]);
    }

    #[test]
    fn recently_used_actions_rank_first_among_equals() {
        // "go" matches every page through its keywords equally.
        let at = |rows: &[Row], id| rows.iter().position(|r| r.entry == Entry::Action(id)).unwrap();
        let plain = root_rows("go", &playing(), &[], None);
        assert!(at(&plain, ActionId::GoHome) < at(&plain, ActionId::GoSearch));
        let recents = vec![Recent::Action { key: "go-search".into() }];
        let lifted = root_rows("go", &playing(), &recents, None);
        assert!(at(&lifted, ActionId::GoSearch) < at(&lifted, ActionId::GoHome));
    }

    #[test]
    fn keywords_match_without_highlight() {
        let rows = root_rows("skip", &playing(), &[], None);
        assert_eq!(rows[0].entry, Entry::Action(ActionId::Next));
        assert!(rows[0].hits.is_empty());
    }

    #[test]
    fn choices_mark_the_current_one_and_filter() {
        let state = ChoiceState { sleep: Some(SleepChoice::Minutes(30)), speed: 1.0, theme: "tape", appearance: "system", ..Default::default() };
        let rows = choice_rows(ChoicePage::Sleep, "", &state);
        assert_eq!(rows.len(), SLEEP_CHOICES.len());
        assert!(rows.iter().find(|r| r.checked).is_some_and(|r| r.entry == Entry::Choice(Choice::Sleep(SleepChoice::Minutes(30)))));
        let rows = choice_rows(ChoicePage::Speed, "", &state);
        assert!(rows.iter().find(|r| r.checked).is_some_and(|r| r.title == "1×"));
        let rows = choice_rows(ChoicePage::Theme, "dark", &state);
        assert_eq!(rows[0].entry, Entry::Choice(Choice::Appearance("dark")));
        let rows = choice_rows(ChoicePage::Equalizer, "", &state);
        assert!(rows[0].checked, "Off while the equalizer is off");
    }

    #[test]
    fn tab_moves_between_section_tops() {
        let rows = root_rows("", &playing(), &[], None);
        let second = next_section(&rows, 0, false);
        assert_ne!(rows[second].section, rows[0].section);
        assert_eq!(next_section(&rows, second + 1, true), second, "back to the section's top");
        assert_eq!(next_section(&rows, second, true), 0);
        // Wraps from the last section to the first.
        let last = rows.len() - 1;
        assert_eq!(next_section(&rows, last, false), 0);
    }
}
