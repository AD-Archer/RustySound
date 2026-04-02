use super::artist_links::{parse_artist_names, resolve_artist_id_for_name, ArtistNameLinks};
use super::home_layout::{
    parse_home_layout_settings, serialize_home_layout_settings, HomeAlbumSectionConfig,
    HomeAlbumSource, HomeFeedLoadProfile, HomeLayoutSettings, HomeQuickPicksLayout,
    HomeQuickPicksSize, HomeQuickPlayAction, HomeSongSectionConfig, HomeSongSource,
    HomeSortDirection, HomeTopStripMode,
};
use crate::api::*;
use crate::components::audio_manager::{
    apply_collection_shuffle_mode, assign_collection_queue_meta,
};
use crate::components::{
    ios_audio_log_snapshot, ios_diag_log, AddIntent, AddMenuController, AppView, HomeFeedState,
    HomeRefreshSignal, Icon, Navigation,
};
use crate::db::{save_settings, AppSettings};
use crate::offline_audio::{
    download_songs_batch, is_album_downloaded, is_song_downloaded, mark_collection_downloaded,
    prefetch_song_audio, sync_downloaded_collection_members,
};
use dioxus::prelude::*;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use uuid::Uuid;

const HOME_SECTION_BASE_COUNT: usize = 9;
const HOME_SECTION_LOAD_STEP: usize = 6;
const HOME_LOADING_FORCE_UNBLOCK_MS: u64 = 12_000;

fn anchored_menu_style(
    anchor_x: f64,
    anchor_y: f64,
    menu_width: f64,
    menu_max_height: f64,
) -> String {
    let preferred_top = (anchor_y + 8.0).max(8.0);
    let preferred_left = (anchor_x - menu_width).max(4.0);
    format!(
        "top: clamp(8px, {:.1}px, calc(100vh - {:.1}px - 8px)); left: clamp(4px, {:.1}px, calc(100vw - {:.1}px - 4px)); max-height: min({:.1}px, calc(100vh - 16px)); overflow-y: auto;",
        preferred_top, menu_max_height, preferred_left, menu_width, menu_max_height
    )
}

fn loading_progress_percent(progress: f32) -> u32 {
    (progress.clamp(0.0, 1.0) * 100.0).round() as u32
}

#[component]
fn LoadingProgressBar(progress: f32, stage: String) -> Element {
    let percent = loading_progress_percent(progress);
    rsx! {
        div { class: "w-full space-y-2",
            div { class: "flex items-center justify-between gap-3 text-xs text-zinc-500",
                p { class: "truncate text-left", "{stage}" }
                p { class: "shrink-0 font-medium text-zinc-400", "{percent}%" }
            }
            div { class: "h-2 overflow-hidden rounded-full bg-zinc-800/80",
                div {
                    class: "h-full rounded-full bg-gradient-to-r from-emerald-500 via-emerald-400 to-teal-300 transition-[width] duration-500 ease-out",
                    style: format!("width: {percent}%;"),
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn home_loading_log_poll_sleep() {
    tokio::time::sleep(std::time::Duration::from_millis(350)).await;
}

#[cfg(target_arch = "wasm32")]
async fn home_loading_log_poll_sleep() {
    gloo_timers::future::TimeoutFuture::new(350).await;
}

#[cfg(not(target_arch = "wasm32"))]
async fn home_quick_picks_grid_poll_sleep() {
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
}

#[cfg(target_arch = "wasm32")]
async fn home_quick_picks_grid_poll_sleep() {
    gloo_timers::future::TimeoutFuture::new(700).await;
}

#[cfg(not(target_arch = "wasm32"))]
fn home_now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(target_arch = "wasm32")]
fn home_now_millis() -> u64 {
    js_sys::Date::now().max(0.0).round() as u64
}

fn section_direction_is_asc(direction: HomeSortDirection) -> bool {
    matches!(direction, HomeSortDirection::Asc)
}

fn dedupe_albums(items: Vec<Album>) -> Vec<Album> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for album in items {
        let key = format!("{}::{}", album.server_id, album.id);
        if seen.insert(key) {
            output.push(album);
        }
    }
    output
}

fn dedupe_songs(items: Vec<Song>) -> Vec<Song> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for song in items {
        let key = format!("{}::{}", song.server_id, song.id);
        if seen.insert(key) {
            output.push(song);
        }
    }
    output
}

fn stable_hash_u64<T: Hash>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn apply_album_direction(mut items: Vec<Album>, direction: HomeSortDirection) -> Vec<Album> {
    if section_direction_is_asc(direction) {
        items.reverse();
    }
    items
}

fn apply_song_direction(mut items: Vec<Song>, direction: HomeSortDirection) -> Vec<Song> {
    if section_direction_is_asc(direction) {
        items.reverse();
    }
    items
}

fn compare_direction(ord: Ordering, direction: HomeSortDirection) -> Ordering {
    if section_direction_is_asc(direction) {
        ord
    } else {
        ord.reverse()
    }
}

fn build_album_section_items(
    source: HomeAlbumSource,
    direction: HomeSortDirection,
    min_rating: u8,
    recent: &[Album],
    most_played: &[Album],
) -> Vec<Album> {
    let merged = dedupe_albums(
        recent
            .iter()
            .cloned()
            .chain(most_played.iter().cloned())
            .collect(),
    );

    let mut items = match source {
        HomeAlbumSource::RecentlyAdded => recent.to_vec(),
        HomeAlbumSource::RecentlyPlayed => apply_album_direction(most_played.to_vec(), direction),
        HomeAlbumSource::MostPlayed => most_played.to_vec(),
        HomeAlbumSource::AtoZ => {
            let mut sorted = merged;
            sorted.sort_by(|left, right| {
                compare_direction(
                    left.name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase())
                        .then_with(|| left.name.cmp(&right.name)),
                    direction,
                )
            });
            sorted
        }
        HomeAlbumSource::Rating => {
            let mut sorted = merged;
            sorted.sort_by(|left, right| {
                compare_direction(
                    left.user_rating
                        .unwrap_or(0)
                        .cmp(&right.user_rating.unwrap_or(0))
                        .then_with(|| {
                            left.name
                                .to_ascii_lowercase()
                                .cmp(&right.name.to_ascii_lowercase())
                        }),
                    direction,
                )
            });
            sorted
        }
        HomeAlbumSource::Random => {
            let mut sorted = merged;
            sorted.sort_by(|left, right| {
                compare_direction(
                    stable_hash_u64(&format!("{}::{}", left.server_id, left.id)).cmp(
                        &stable_hash_u64(&format!("{}::{}", right.server_id, right.id)),
                    ),
                    direction,
                )
            });
            sorted
        }
    };

    if matches!(
        source,
        HomeAlbumSource::RecentlyAdded | HomeAlbumSource::MostPlayed
    ) {
        items = apply_album_direction(items, direction);
    }

    if matches!(source, HomeAlbumSource::Rating) && min_rating > 0 {
        let minimum = min_rating.min(5) as u32;
        items.retain(|album| album.user_rating.unwrap_or(0) >= minimum);
    }

    items
}

fn build_song_section_items(
    source: HomeSongSource,
    direction: HomeSortDirection,
    min_rating: u8,
    recent: &[Song],
    most_played: &[Song],
    random: &[Song],
    quick_picks: &[Song],
) -> Vec<Song> {
    let merged = dedupe_songs(
        recent
            .iter()
            .cloned()
            .chain(most_played.iter().cloned())
            .chain(random.iter().cloned())
            .chain(quick_picks.iter().cloned())
            .collect(),
    );

    let mut items = match source {
        HomeSongSource::MostPlayed => most_played.to_vec(),
        HomeSongSource::RecentlyPlayed => recent.to_vec(),
        HomeSongSource::Random => random.to_vec(),
        HomeSongSource::QuickPicks => quick_picks.to_vec(),
        HomeSongSource::AtoZ => {
            let mut sorted = merged;
            sorted.sort_by(|left, right| {
                compare_direction(
                    left.title
                        .to_ascii_lowercase()
                        .cmp(&right.title.to_ascii_lowercase())
                        .then_with(|| left.title.cmp(&right.title)),
                    direction,
                )
            });
            sorted
        }
        HomeSongSource::Rating => {
            let mut sorted = merged;
            sorted.sort_by(|left, right| {
                compare_direction(
                    left.user_rating
                        .unwrap_or(0)
                        .cmp(&right.user_rating.unwrap_or(0))
                        .then_with(|| {
                            left.title
                                .to_ascii_lowercase()
                                .cmp(&right.title.to_ascii_lowercase())
                        }),
                    direction,
                )
            });
            sorted
        }
    };

    if matches!(
        source,
        HomeSongSource::MostPlayed
            | HomeSongSource::RecentlyPlayed
            | HomeSongSource::Random
            | HomeSongSource::QuickPicks
    ) {
        items = apply_song_direction(items, direction);
    }

    if matches!(source, HomeSongSource::Rating) && min_rating > 0 {
        let minimum = min_rating.min(5) as u32;
        items.retain(|song| song.user_rating.unwrap_or(0) >= minimum);
    }

    items
}

fn quick_play_action_title(action: HomeQuickPlayAction) -> &'static str {
    match action {
        HomeQuickPlayAction::AllSongs => "All Songs",
        HomeQuickPlayAction::Favorites => "Favorites",
        HomeQuickPlayAction::Downloads => "Downloads",
        HomeQuickPlayAction::Playlists => "Playlists",
        HomeQuickPlayAction::RandomMix => "Random Mix",
        HomeQuickPlayAction::RadioStations => "Radio Stations",
        HomeQuickPlayAction::AllAlbums => "All Albums",
        HomeQuickPlayAction::Artists => "Artists",
        HomeQuickPlayAction::Bookmarks => "Bookmarks",
        HomeQuickPlayAction::Stats => "Stats",
        HomeQuickPlayAction::Queue => "Queue",
    }
}

fn quick_play_action_icon(action: HomeQuickPlayAction) -> &'static str {
    match action {
        HomeQuickPlayAction::AllSongs => "music",
        HomeQuickPlayAction::Favorites => "heart",
        HomeQuickPlayAction::Downloads => "download",
        HomeQuickPlayAction::Playlists => "playlist",
        HomeQuickPlayAction::RandomMix => "shuffle",
        HomeQuickPlayAction::RadioStations => "radio",
        HomeQuickPlayAction::AllAlbums => "album",
        HomeQuickPlayAction::Artists => "artist",
        HomeQuickPlayAction::Bookmarks => "bookmark",
        HomeQuickPlayAction::Stats => "bars",
        HomeQuickPlayAction::Queue => "queue",
    }
}

fn quick_play_action_gradient(action: HomeQuickPlayAction) -> &'static str {
    match action {
        HomeQuickPlayAction::AllSongs => "from-sky-600 to-cyan-600",
        HomeQuickPlayAction::Favorites => "from-rose-600 to-pink-600",
        HomeQuickPlayAction::Downloads => "from-indigo-500 to-blue-600",
        HomeQuickPlayAction::Playlists => "from-amber-600 to-orange-600",
        HomeQuickPlayAction::RandomMix => "from-fuchsia-600 to-violet-600",
        HomeQuickPlayAction::RadioStations => "from-emerald-600 to-teal-600",
        HomeQuickPlayAction::AllAlbums => "from-amber-500 to-rose-500",
        HomeQuickPlayAction::Artists => "from-purple-600 to-indigo-600",
        HomeQuickPlayAction::Bookmarks => "from-teal-600 to-emerald-500",
        HomeQuickPlayAction::Stats => "from-blue-600 to-indigo-700",
        HomeQuickPlayAction::Queue => "from-zinc-600 to-slate-700",
    }
}

fn quick_play_action_view(action: HomeQuickPlayAction) -> AppView {
    match action {
        HomeQuickPlayAction::AllSongs => AppView::SongsView {},
        HomeQuickPlayAction::Favorites => AppView::FavoritesView {},
        HomeQuickPlayAction::Downloads => AppView::DownloadsView {},
        HomeQuickPlayAction::Playlists => AppView::PlaylistsView {},
        HomeQuickPlayAction::RandomMix => AppView::RandomView {},
        HomeQuickPlayAction::RadioStations => AppView::RadioView {},
        HomeQuickPlayAction::AllAlbums => AppView::Albums {},
        HomeQuickPlayAction::Artists => AppView::ArtistsView {},
        HomeQuickPlayAction::Bookmarks => AppView::BookmarksView {},
        HomeQuickPlayAction::Stats => AppView::StatsView {},
        HomeQuickPlayAction::Queue => AppView::QueueView {},
    }
}

fn album_section_key(id: &str) -> String {
    format!("album::{id}")
}

fn song_section_key(id: &str) -> String {
    format!("song::{id}")
}

fn even_card_count(count: usize) -> usize {
    if count % 2 != 0 {
        count.saturating_sub(1)
    } else {
        count
    }
}

fn default_album_section_title(source: HomeAlbumSource) -> &'static str {
    match source {
        HomeAlbumSource::RecentlyAdded => "Recently Added Albums",
        HomeAlbumSource::RecentlyPlayed => "Recently Played Albums",
        HomeAlbumSource::MostPlayed => "Most Played Albums",
        HomeAlbumSource::AtoZ => "A-Z Albums",
        HomeAlbumSource::Rating => "Top Rated Albums",
        HomeAlbumSource::Random => "Random Albums",
    }
}

fn default_song_section_title(source: HomeSongSource) -> &'static str {
    match source {
        HomeSongSource::MostPlayed => "Most Played Songs",
        HomeSongSource::RecentlyPlayed => "Recently Played Songs",
        HomeSongSource::Random => "Random Songs",
        HomeSongSource::AtoZ => "A-Z Songs",
        HomeSongSource::Rating => "Top Rated Songs",
        HomeSongSource::QuickPicks => "Quick Pick Songs",
    }
}

fn should_autoname_section_title(current_title: &str, previous_default: &str) -> bool {
    let title = current_title.trim();
    title.is_empty() || title.eq_ignore_ascii_case(previous_default)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuickPicksVisibleAmount {
    Small,
    Medium,
    Large,
}

impl QuickPicksVisibleAmount {
    fn as_value(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    fn from_value(value: &str) -> Self {
        match value {
            "small" => Self::Small,
            "large" => Self::Large,
            _ => Self::Medium,
        }
    }
}

fn quick_picks_visible_count_for_amount(amount: QuickPicksVisibleAmount) -> usize {
    match amount {
        QuickPicksVisibleAmount::Small => 14,
        QuickPicksVisibleAmount::Medium => 25,
        QuickPicksVisibleAmount::Large => 40,
    }
}

fn quick_picks_visible_amount_from_count(count: usize) -> QuickPicksVisibleAmount {
    let small_delta = count.abs_diff(14);
    let medium_delta = count.abs_diff(25);
    let large_delta = count.abs_diff(40);

    if small_delta <= medium_delta && small_delta <= large_delta {
        QuickPicksVisibleAmount::Small
    } else if large_delta < medium_delta {
        QuickPicksVisibleAmount::Large
    } else {
        QuickPicksVisibleAmount::Medium
    }
}

fn normalize_quick_picks_requested_count(count: usize) -> usize {
    let mut normalized = count.clamp(2, 48);
    if normalized % 2 != 0 {
        normalized = if normalized < 48 {
            normalized.saturating_add(1)
        } else {
            normalized.saturating_sub(1)
        };
    }
    normalized.clamp(2, 48)
}

fn snap_quick_picks_requested_to_grid(requested_visible: usize, columns: usize) -> usize {
    let cols = columns.max(1);
    let target = normalize_quick_picks_requested_count(requested_visible)
        .max(cols)
        .min(48);
    let lower_rows = (target / cols).max(1);
    let lower = lower_rows.saturating_mul(cols).min(48);
    let upper_rows = ((target + cols - 1) / cols).max(1);
    let upper = upper_rows.saturating_mul(cols).min(48);
    if target.abs_diff(lower) <= target.abs_diff(upper) {
        lower
    } else {
        upper
    }
}

fn quick_picks_grid_visible_limit_for_loaded(
    requested_visible: usize,
    columns: usize,
    total_items: usize,
) -> usize {
    if total_items == 0 {
        return 0;
    }

    let cols = columns.max(1);
    let cap = requested_visible.min(total_items);
    let rows = cap / cols;
    if rows > 0 {
        return rows.saturating_mul(cols).min(total_items);
    }

    cap.clamp(1, total_items)
}

fn quick_picks_list_visible_limit_for_loaded(
    requested_visible: usize,
    total_items: usize,
) -> usize {
    if total_items == 0 {
        return 0;
    }

    let mut visible = normalize_quick_picks_requested_count(requested_visible).min(total_items);
    if visible == 0 {
        visible = total_items.min(2);
    }
    visible.clamp(1, total_items)
}

fn quick_picks_target_columns(size: HomeQuickPicksSize) -> usize {
    size.target_columns()
}

fn quick_picks_card_min_px(size: HomeQuickPicksSize) -> usize {
    match size {
        HomeQuickPicksSize::Small => 112,
        HomeQuickPicksSize::Medium => 136,
        HomeQuickPicksSize::Large => 164,
    }
}

fn quick_picks_list_batch_size(size: HomeQuickPicksSize) -> usize {
    match size {
        HomeQuickPicksSize::Small => 14,
        HomeQuickPicksSize::Medium => 10,
        HomeQuickPicksSize::Large => 8,
    }
}

fn new_album_section() -> HomeAlbumSectionConfig {
    let source = HomeAlbumSource::MostPlayed;
    HomeAlbumSectionConfig {
        id: format!("album-{}", Uuid::new_v4()),
        title: default_album_section_title(source).to_string(),
        enabled: true,
        source,
        direction: HomeSortDirection::Desc,
        min_rating: 0,
        initial_visible: HOME_SECTION_BASE_COUNT as u8,
        load_step: HOME_SECTION_LOAD_STEP as u8,
    }
}

fn new_song_section() -> HomeSongSectionConfig {
    let source = HomeSongSource::MostPlayed;
    HomeSongSectionConfig {
        id: format!("song-{}", Uuid::new_v4()),
        title: default_song_section_title(source).to_string(),
        enabled: true,
        source,
        direction: HomeSortDirection::Desc,
        min_rating: 0,
        initial_visible: HOME_SECTION_BASE_COUNT as u8,
        load_step: HOME_SECTION_LOAD_STEP as u8,
    }
}

fn profile_section_defaults(profile: HomeFeedLoadProfile) -> (usize, usize) {
    match profile {
        HomeFeedLoadProfile::Conservative => (6, 4),
        HomeFeedLoadProfile::Standard => (9, 6),
        HomeFeedLoadProfile::Super => (12, 8),
    }
}

fn section_auto_budget(profile: HomeFeedLoadProfile, total_items: usize) -> (usize, usize) {
    let (base, step) = profile_section_defaults(profile);
    if total_items == 0 {
        return (0, step.max(1));
    }

    let initial_visible = base.min(total_items);
    let remaining = total_items.saturating_sub(initial_visible);
    let load_step = if remaining == 0 {
        step.max(1)
    } else {
        step.min(remaining.max(1)).max(1)
    };

    (initial_visible, load_step)
}

#[component]
pub fn HomeView() -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let mut now_playing = use_context::<Signal<Option<Song>>>();
    let mut queue = use_context::<Signal<Vec<Song>>>();
    let mut queue_index = use_context::<Signal<usize>>();
    let mut is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let home_refresh_generation = use_context::<HomeRefreshSignal>().0;
    let app_settings = use_context::<Signal<AppSettings>>();

    let home_feed = use_context::<HomeFeedState>();
    let recent_albums = home_feed.recent_albums;
    let most_played_albums = home_feed.most_played_albums;
    let recently_played_songs = home_feed.recently_played_songs;
    let most_played_songs = home_feed.most_played_songs;
    let random_songs = home_feed.random_songs;
    let quick_picks = home_feed.quick_picks;
    let home_loading_progress = home_feed.progress;
    let home_loading_status = home_feed.status;
    let mut ios_loading_log_lines = use_signal(Vec::<String>::new);
    let mut ios_loading_log_poll_generation = use_signal(|| 0u64);
    let mut quick_picks_grid_poll_generation = use_signal(|| 0u64);
    let mut quick_picks_runtime_columns = use_signal(|| 0usize);
    let mut home_loading_started_at_ms = use_signal(|| None::<u64>);
    let mut home_loading_elapsed_ms = use_signal(|| 0u64);
    let mut home_loading_force_unblocked = use_signal(|| false);
    let mut section_visible_overrides = use_signal(HashMap::<String, usize>::new);
    let show_home_editor = use_signal(|| false);
    let mut home_layout =
        use_signal(|| parse_home_layout_settings(&app_settings().home_layout_json));
    let mut home_layout_draft = use_signal(|| home_layout());

    use_effect(move || {
        ios_diag_log("home.view.mount", "HomeView mounted");
    });

    use_effect(move || {
        let parsed = parse_home_layout_settings(&app_settings().home_layout_json);
        if parsed != home_layout() {
            home_layout.set(parsed.clone());
            if !show_home_editor() {
                home_layout_draft.set(parsed);
            }
        }
    });

    use_effect(move || {
        let _ = home_layout();
        section_visible_overrides.set(HashMap::new());
    });

    use_effect(move || {
        if !show_home_editor() {
            return;
        }
        let _ = document::eval(
            r#"
(() => {
  const editor = document.getElementById("home-layout-editor");
  if (!editor) return false;
  editor.scrollIntoView({ behavior: "smooth", block: "start" });
  return true;
})();
"#,
        );
    });

    let has_servers = servers().iter().any(|s| s.active);
    let is_home_album_loading =
        has_servers && (recent_albums().is_none() || most_played_albums().is_none());
    let show_home_album_overlay = is_home_album_loading && !home_loading_force_unblocked();
    let show_ios_loading_logs = cfg!(all(not(target_arch = "wasm32"), target_os = "ios"));

    use_effect(move || {
        let recent_album_count = recent_albums().as_ref().map(Vec::len);
        let most_played_album_count = most_played_albums().as_ref().map(Vec::len);
        let recent_song_count = recently_played_songs().as_ref().map(Vec::len);
        let most_played_song_count = most_played_songs().as_ref().map(Vec::len);
        let random_song_count = random_songs().as_ref().map(Vec::len);
        let quick_pick_count = quick_picks().as_ref().map(Vec::len);

        ios_diag_log(
            "home.view.state",
            &format!(
                "servers_active={} overlay={} recent_albums={} most_played_albums={} recent_songs={} most_played_songs={} random_songs={} quick_picks={}",
                has_servers,
                show_home_album_overlay,
                recent_album_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                most_played_album_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                recent_song_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                most_played_song_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                random_song_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
                quick_pick_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "pending".to_string()),
            ),
        );
    });

    use_effect(move || {
        if !is_home_album_loading {
            home_loading_started_at_ms.set(None);
            home_loading_elapsed_ms.set(0);
            home_loading_force_unblocked.set(false);
            return;
        }

        if home_loading_started_at_ms().is_some() {
            return;
        }

        let started_at = home_now_millis();
        home_loading_started_at_ms.set(Some(started_at));
        home_loading_elapsed_ms.set(0);
        home_loading_force_unblocked.set(false);
        ios_diag_log(
            "home.view.load",
            &format!("album loading overlay shown at={started_at}"),
        );
    });

    use_effect(move || {
        if !cfg!(all(not(target_arch = "wasm32"), target_os = "ios")) {
            ios_loading_log_lines.set(Vec::new());
            return;
        }

        if !is_home_album_loading {
            ios_loading_log_lines.set(Vec::new());
            ios_loading_log_poll_generation
                .with_mut(|generation| *generation = generation.saturating_add(1));
            return;
        }

        ios_loading_log_poll_generation
            .with_mut(|generation| *generation = generation.saturating_add(1));
        let generation = *ios_loading_log_poll_generation.peek();
        let mut ios_loading_log_lines = ios_loading_log_lines.clone();
        let ios_loading_log_poll_generation = ios_loading_log_poll_generation.clone();
        let mut home_loading_started_at_ms = home_loading_started_at_ms.clone();
        let mut home_loading_elapsed_ms = home_loading_elapsed_ms.clone();
        let mut home_loading_force_unblocked = home_loading_force_unblocked.clone();
        spawn(async move {
            loop {
                ios_loading_log_lines.set(ios_audio_log_snapshot(16));
                let started_at_ms = match home_loading_started_at_ms() {
                    Some(value) => value,
                    None => {
                        let now = home_now_millis();
                        home_loading_started_at_ms.set(Some(now));
                        now
                    }
                };
                let elapsed_ms = home_now_millis().saturating_sub(started_at_ms);
                home_loading_elapsed_ms.set(elapsed_ms);

                if !home_loading_force_unblocked() && elapsed_ms >= HOME_LOADING_FORCE_UNBLOCK_MS {
                    home_loading_force_unblocked.set(true);
                    ios_diag_log(
                        "home.view.load",
                        &format!(
                            "force dismiss album loading overlay after {elapsed_ms}ms timeout"
                        ),
                    );
                }
                home_loading_log_poll_sleep().await;
                if *ios_loading_log_poll_generation.peek() != generation {
                    break;
                }
            }
        });
    });

    let ios_loading_logs_preview = ios_loading_log_lines();
    let home_loading_elapsed_ms = home_loading_elapsed_ms();
    let home_loading_progress_value = home_loading_progress();
    let home_loading_status_text =
        home_loading_status().unwrap_or_else(|| "Loading Home feed".to_string());
    let home_loading_recent_count_value = recent_albums().as_ref().map(Vec::len);
    let home_loading_most_played_count_value = most_played_albums().as_ref().map(Vec::len);
    let home_loading_error_text = None::<String>;
    let on_unblock_home_loading = {
        let mut home_loading_force_unblocked = home_loading_force_unblocked.clone();
        move |_| {
            home_loading_force_unblocked.set(true);
            ios_diag_log("home.view.load", "manual dismiss of album loading overlay");
        }
    };
    let on_refresh_home_feed = {
        let mut home_refresh_generation = home_refresh_generation.clone();
        let mut home_loading_force_unblocked = home_loading_force_unblocked.clone();
        move |_| {
            home_loading_force_unblocked.set(false);
            home_refresh_generation.with_mut(|generation| {
                *generation = generation.saturating_add(1);
            });
            ios_diag_log("home.view.refresh", "manual Home feed refresh requested");
        }
    };
    let on_open_home_editor = {
        let mut show_home_editor = show_home_editor.clone();
        let mut home_layout_draft = home_layout_draft.clone();
        let home_layout = home_layout.clone();
        move |_| {
            home_layout_draft.set(home_layout());
            show_home_editor.set(true);
        }
    };
    let on_cancel_home_editor = {
        let mut show_home_editor = show_home_editor.clone();
        let mut home_layout_draft = home_layout_draft.clone();
        let home_layout = home_layout.clone();
        move |_| {
            home_layout_draft.set(home_layout());
            show_home_editor.set(false);
        }
    };
    let on_apply_home_editor = {
        let mut show_home_editor = show_home_editor.clone();
        let mut home_layout = home_layout.clone();
        let home_layout_draft = home_layout_draft.clone();
        let mut app_settings = app_settings.clone();
        let mut section_visible_overrides = section_visible_overrides.clone();
        let mut home_refresh_generation = home_refresh_generation.clone();
        move |_| {
            let normalized = home_layout_draft().normalized();
            let previous_profile =
                HomeFeedLoadProfile::from_storage(&app_settings().home_feed_load_profile);
            let next_profile = normalized.fetch_profile;
            let serialized = serialize_home_layout_settings(&normalized);

            home_layout.set(normalized.clone());
            section_visible_overrides.set(HashMap::new());
            show_home_editor.set(false);

            let mut settings = app_settings();
            settings.home_layout_json = serialized;
            settings.home_feed_load_profile = next_profile.as_storage().to_string();
            app_settings.set(settings.clone());
            spawn(async move {
                let _ = save_settings(settings).await;
            });

            if previous_profile != next_profile {
                home_refresh_generation.with_mut(|generation| {
                    *generation = generation.saturating_add(1);
                });
            }
        }
    };
    let on_reset_home_editor_defaults = {
        let mut home_layout_draft = home_layout_draft.clone();
        move |_| {
            home_layout_draft.set(HomeLayoutSettings::default());
        }
    };

    let editor_open = show_home_editor();
    let layout_draft_snapshot = home_layout_draft();
    let layout_snapshot = if editor_open {
        layout_draft_snapshot.clone().normalized()
    } else {
        home_layout()
    };
    let recent_album_items = recent_albums().unwrap_or_default();
    let most_played_album_items = most_played_albums().unwrap_or_default();
    let recent_song_items = recently_played_songs().unwrap_or_default();
    let most_played_song_items = most_played_songs().unwrap_or_default();
    let random_song_items = random_songs().unwrap_or_default();
    let quick_pick_items = quick_picks().unwrap_or_default();
    let visible_overrides_snapshot = section_visible_overrides();

    use_effect(move || {
        let editor_open = show_home_editor();
        let layout_for_measure = if editor_open {
            home_layout_draft().normalized()
        } else {
            home_layout()
        };
        let quick_picks_grid_measure_enabled = layout_for_measure.quick_picks.enabled
            && matches!(
                layout_for_measure.quick_picks.layout,
                HomeQuickPicksLayout::Grid
            );

        quick_picks_grid_poll_generation
            .with_mut(|generation| *generation = generation.saturating_add(1));
        let generation = *quick_picks_grid_poll_generation.peek();

        if !quick_picks_grid_measure_enabled {
            quick_picks_runtime_columns.set(0);
            return;
        }

        let mut quick_picks_runtime_columns = quick_picks_runtime_columns.clone();
        let quick_picks_grid_poll_generation = quick_picks_grid_poll_generation.clone();
        spawn(async move {
            loop {
                let eval = document::eval(
                    r#"
return (function () {
  const grid = document.getElementById("home-quick-picks-grid");
  if (!grid) return 0;
  const template = (window.getComputedStyle(grid).gridTemplateColumns || "").trim();
  if (!template) return 0;
  const cols = template.split(/\s+/).filter(Boolean).length;
  return Number.isFinite(cols) ? cols : 0;
})();
"#,
                );

                if let Ok(columns) = eval.join::<usize>().await {
                    if columns > 0 {
                        if *quick_picks_runtime_columns.peek() != columns {
                            quick_picks_runtime_columns.set(columns);
                        }
                    }
                }

                home_quick_picks_grid_poll_sleep().await;
                if *quick_picks_grid_poll_generation.peek() != generation {
                    break;
                }
            }
        });
    });

    let top_album_items = build_album_section_items(
        layout_snapshot.top_album_source,
        layout_snapshot.top_album_direction,
        0,
        &recent_album_items,
        &most_played_album_items,
    );
    let (top_album_visible, _) =
        section_auto_budget(layout_snapshot.fetch_profile, top_album_items.len());
    let top_album_display_count = top_album_visible.min(12).min(top_album_items.len());

    let quick_play_limit =
        (layout_snapshot.quick_play.rows as usize) * (layout_snapshot.quick_play.columns as usize);
    let quick_play_actions: Vec<HomeQuickPlayAction> = layout_snapshot
        .quick_play
        .actions
        .iter()
        .copied()
        .take(quick_play_limit.max(1))
        .collect();
    let quick_only_display_count = even_card_count(quick_play_actions.len());
    let album_only_display_count = even_card_count(top_album_display_count);
    let (mixed_quick_display_count, mixed_album_display_count, mixed_total_display_count) = {
        let mut mixed_quick_count = if layout_snapshot.quick_play.enabled {
            quick_play_actions.len()
        } else {
            0
        };
        let mut mixed_album_count = top_album_display_count;
        if (mixed_quick_count + mixed_album_count) > 0
            && (mixed_quick_count + mixed_album_count) % 2 != 0
        {
            if mixed_album_count > 0 {
                mixed_album_count = mixed_album_count.saturating_sub(1);
            } else if mixed_quick_count > 0 {
                mixed_quick_count = mixed_quick_count.saturating_sub(1);
            }
        }
        let mixed_total = mixed_quick_count + mixed_album_count;
        (mixed_quick_count, mixed_album_count, mixed_total)
    };

    let album_sections_render: Vec<(HomeAlbumSectionConfig, String, Vec<Album>, usize, usize)> =
        layout_snapshot
            .album_sections
            .iter()
            .filter(|section| section.enabled)
            .map(|section| {
                let items = build_album_section_items(
                    section.source,
                    section.direction,
                    section.min_rating,
                    &recent_album_items,
                    &most_played_album_items,
                );
                let key = album_section_key(&section.id);
                let (default_visible, load_step) =
                    section_auto_budget(layout_snapshot.fetch_profile, items.len());
                let visible = visible_overrides_snapshot
                    .get(&key)
                    .copied()
                    .unwrap_or(default_visible)
                    .min(items.len());
                (section.clone(), key, items, visible, load_step)
            })
            .collect();

    let song_sections_render: Vec<(HomeSongSectionConfig, String, Vec<Song>, usize, usize)> =
        layout_snapshot
            .song_sections
            .iter()
            .filter(|section| section.enabled)
            .map(|section| {
                let items = build_song_section_items(
                    section.source,
                    section.direction,
                    section.min_rating,
                    &recent_song_items,
                    &most_played_song_items,
                    &random_song_items,
                    &quick_pick_items,
                );
                let key = song_section_key(&section.id);
                let (default_visible, load_step) =
                    section_auto_budget(layout_snapshot.fetch_profile, items.len());
                let visible = visible_overrides_snapshot
                    .get(&key)
                    .copied()
                    .unwrap_or(default_visible)
                    .min(items.len());
                (section.clone(), key, items, visible, load_step)
            })
            .collect();

    let quick_picks_section_key = "quick_picks::main".to_string();
    let quick_picks_size = layout_snapshot.quick_picks.size;
    let quick_picks_target_cols = quick_picks_target_columns(quick_picks_size);
    let quick_picks_layout_is_grid = matches!(
        layout_snapshot.quick_picks.layout,
        HomeQuickPicksLayout::Grid
    );
    let measured_quick_picks_cols = quick_picks_runtime_columns();
    let quick_picks_grid_cols = if measured_quick_picks_cols > 0 {
        measured_quick_picks_cols
    } else {
        quick_picks_target_cols
    }
    .max(1);
    let quick_picks_requested_visible_from_layout = if quick_picks_layout_is_grid {
        snap_quick_picks_requested_to_grid(
            layout_snapshot.quick_picks.visible_count as usize,
            quick_picks_grid_cols,
        )
    } else {
        normalize_quick_picks_requested_count(layout_snapshot.quick_picks.visible_count as usize)
    };
    let quick_picks_default_visible_limit = quick_picks_requested_visible_from_layout;
    let quick_picks_requested_visible_raw = if editor_open {
        quick_picks_default_visible_limit
    } else {
        visible_overrides_snapshot
            .get(&quick_picks_section_key)
            .copied()
            .unwrap_or(quick_picks_default_visible_limit)
    };
    let quick_picks_requested_visible = if quick_picks_layout_is_grid {
        snap_quick_picks_requested_to_grid(quick_picks_requested_visible_raw, quick_picks_grid_cols)
    } else {
        normalize_quick_picks_requested_count(quick_picks_requested_visible_raw)
    };
    let quick_picks_visible_limit = if quick_picks_layout_is_grid {
        quick_picks_grid_visible_limit_for_loaded(
            quick_picks_requested_visible,
            quick_picks_grid_cols,
            quick_pick_items.len(),
        )
    } else {
        quick_picks_list_visible_limit_for_loaded(
            quick_picks_requested_visible,
            quick_pick_items.len(),
        )
    };
    let quick_picks_load_step = if quick_picks_layout_is_grid {
        quick_picks_grid_cols
    } else {
        let (_, step) = profile_section_defaults(layout_snapshot.fetch_profile);
        step.max(quick_picks_list_batch_size(quick_picks_size))
    };
    let quick_picks_refresh_target = if quick_picks_layout_is_grid {
        quick_picks_requested_visible.max(quick_picks_requested_visible_from_layout)
    } else {
        normalize_quick_picks_requested_count(
            quick_picks_requested_visible
                .max(quick_picks_requested_visible_from_layout)
                .max(2),
        )
    };
    let quick_picks_needs_refresh = quick_pick_items.len() < quick_picks_refresh_target;

    let quick_picks_display: Vec<Song> = quick_pick_items
        .iter()
        .take(quick_picks_visible_limit)
        .cloned()
        .collect();
    let quick_picks_grid_min_px = quick_picks_card_min_px(quick_picks_size);

    rsx! {
        div { class: "space-y-8 max-w-none",
            if show_home_album_overlay {
                div { class: "fixed inset-0 z-[210] bg-zinc-950/95 backdrop-blur-sm overflow-y-auto px-6 py-8 flex items-center justify-center",
                    div { class: "w-full max-w-lg text-center space-y-4 rounded-2xl border border-zinc-700/70 bg-zinc-950/95 px-5 py-5 shadow-2xl",
                        div { class: "flex items-center justify-center",
                            Icon {
                                name: "loader".to_string(),
                                class: "w-10 h-10 text-emerald-400 animate-spin".to_string(),
                            }
                        }
                        h2 { class: "text-xl font-semibold text-white", "Loading Home" }
                        p { class: "text-sm text-zinc-400",
                            "Fetching albums and preparing your home feed."
                        }
                        LoadingProgressBar {
                            progress: home_loading_progress_value,
                            stage: home_loading_status_text,
                        }
                        div { class: "grid grid-cols-2 gap-3 text-left",
                            div { class: "rounded-xl border border-zinc-800 bg-zinc-900/70 px-3 py-2",
                                p { class: "text-[10px] uppercase tracking-wide text-zinc-500",
                                    "Recent Albums"
                                }
                                p { class: "text-sm font-medium text-white",
                                    match home_loading_recent_count_value {
                                        Some(count) => format!("{count}"),
                                        None => "Pending".to_string(),
                                    }
                                }
                            }
                            div { class: "rounded-xl border border-zinc-800 bg-zinc-900/70 px-3 py-2",
                                p { class: "text-[10px] uppercase tracking-wide text-zinc-500",
                                    "Most Played Albums"
                                }
                                p { class: "text-sm font-medium text-white",
                                    match home_loading_most_played_count_value {
                                        Some(count) => format!("{count}"),
                                        None => "Pending".to_string(),
                                    }
                                }
                            }
                        }
                        p { class: "text-xs text-zinc-500", "Elapsed: {home_loading_elapsed_ms} ms" }
                        if let Some(error_text) = home_loading_error_text {
                            p { class: "text-xs text-amber-300", "{error_text}" }
                        }
                        button {
                            class: "mt-1 px-3 py-2 rounded-lg border border-zinc-600 text-zinc-200 hover:text-white hover:border-zinc-400 transition-colors text-sm",
                            onclick: on_unblock_home_loading,
                            "Continue without blocking"
                        }
                        if show_ios_loading_logs && !ios_loading_logs_preview.is_empty() {
                            div { class: "mt-3 text-left rounded-lg border border-zinc-700/70 bg-zinc-900/70 p-2 max-h-72 overflow-y-auto",
                                p { class: "text-[10px] uppercase tracking-wide text-zinc-500 mb-1",
                                    "iOS Loading Log"
                                }
                                for line in ios_loading_logs_preview.iter() {
                                    p { class: "text-[11px] leading-tight text-zinc-300 font-mono break-all",
                                        "{line}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Welcome header
            header { class: "page-header",
                h1 { class: "page-title", "Good evening" }
                p { class: "page-subtitle",
                    if has_servers {
                        "Welcome back. Here's what's new in your library."
                    } else {
                        "Connect a Navidrome server to get started."
                    }
                }
            }

            if !has_servers {
                // Empty state - no servers
                div { class: "flex flex-col items-center justify-center py-20",
                    div { class: "w-20 h-20 rounded-2xl bg-zinc-800/50 flex items-center justify-center mb-6",
                        Icon {
                            name: "server".to_string(),
                            class: "w-10 h-10 text-zinc-500".to_string(),
                        }
                    }
                    h2 { class: "text-xl font-semibold text-white mb-2", "No servers connected" }
                    p { class: "text-zinc-400 text-center max-w-md mb-6",
                        "Add your Navidrome server to start streaming your music collection."
                    }
                    button {
                        class: "px-6 py-3 bg-emerald-500 hover:bg-emerald-400 text-white font-medium rounded-xl transition-colors",
                        onclick: {
                            let nav = navigation.clone();
                            move |_| nav.navigate_to(AppView::SettingsView {})
                        },
                        "Add Server"
                    }
                }
            } else {
                if matches!(
                    layout_snapshot.top_strip_mode,
                    HomeTopStripMode::QuickPlay
                ) && layout_snapshot.quick_play.enabled
                {
                    div { class: "flex justify-center mb-8",
                        if quick_only_display_count == 0 {
                            div { class: "w-full max-w-7xl rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-4 text-sm text-zinc-400",
                                "No quick play cards selected."
                            }
                        } else {
                            div {
                                class: "grid gap-3 w-full max-w-7xl",
                                style: format!(
                                    "grid-template-columns: repeat({}, minmax(0, 1fr));",
                                    layout_snapshot
                                        .quick_play
                                        .columns
                                        .clamp(1, 6)
                                        .min(quick_only_display_count as u8)
                                        .max(1),
                                ),
                                for action in quick_play_actions.iter().copied().take(quick_only_display_count) {
                                    QuickPlayCard {
                                        title: quick_play_action_title(action).to_string(),
                                        gradient: quick_play_action_gradient(action).to_string(),
                                        icon: quick_play_action_icon(action).to_string(),
                                        onclick: {
                                            let nav = navigation.clone();
                                            move |_| nav.navigate_to(quick_play_action_view(action))
                                        },
                                    }
                                }
                            }
                        }
                    }
                }

                if matches!(
                    layout_snapshot.top_strip_mode,
                    HomeTopStripMode::Mixed
                )
                {
                    div { class: "flex justify-center mb-8",
                        if mixed_total_display_count == 0 {
                            div { class: "w-full max-w-7xl rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-4 text-sm text-zinc-400",
                                "No top strip cards available."
                            }
                        } else {
                            div {
                                class: "grid gap-3 w-full max-w-7xl",
                                style: format!(
                                    "grid-template-columns: repeat({}, minmax(0, 1fr));",
                                    layout_snapshot
                                        .quick_play
                                        .columns
                                        .clamp(1, 6)
                                        .min(mixed_total_display_count as u8)
                                        .max(1),
                                ),
                                for action in quick_play_actions.iter().copied().take(mixed_quick_display_count) {
                                    QuickPlayCard {
                                        title: quick_play_action_title(action).to_string(),
                                        gradient: quick_play_action_gradient(action).to_string(),
                                        icon: quick_play_action_icon(action).to_string(),
                                        onclick: {
                                            let nav = navigation.clone();
                                            move |_| nav.navigate_to(quick_play_action_view(action))
                                        },
                                    }
                                }
                                for album in top_album_items.iter().take(mixed_album_display_count) {
                                    AlbumHighlightCard {
                                        album: album.clone(),
                                        onclick: {
                                            let navigation = navigation.clone();
                                            let album_id = album.id.clone();
                                            let album_server_id = album.server_id.clone();
                                            move |_| {
                                                navigation.navigate_to(AppView::AlbumDetailView {
                                                    album_id: album_id.clone(),
                                                    server_id: album_server_id.clone(),
                                                })
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }

                if matches!(
                    layout_snapshot.top_strip_mode,
                    HomeTopStripMode::AlbumHighlights
                )
                {
                    section { class: "mb-8",
                        div { class: "flex items-center justify-between mb-4",
                            h2 { class: "text-xl font-semibold text-white", "Album Highlights" }
                            button {
                                class: "text-sm text-zinc-400 hover:text-white transition-colors",
                                onclick: {
                                    let nav = navigation.clone();
                                    move |_| nav.navigate_to(AppView::Albums {})
                                },
                                "See all"
                            }
                        }
                        if album_only_display_count == 0 {
                            div { class: "rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-4 text-sm text-zinc-400",
                                "No albums available for this top strip mode yet."
                            }
                        } else {
                            div { class: "flex justify-center",
                                div {
                                    class: "grid gap-3 w-full max-w-7xl",
                                    style: format!(
                                        "grid-template-columns: repeat({}, minmax(0, 1fr));",
                                        layout_snapshot
                                            .quick_play
                                            .columns
                                            .clamp(1, 6)
                                            .min(album_only_display_count as u8)
                                            .max(1),
                                    ),
                                    for album in top_album_items.iter().take(album_only_display_count) {
                                        AlbumHighlightCard {
                                            album: album.clone(),
                                            onclick: {
                                                let navigation = navigation.clone();
                                                let album_id = album.id.clone();
                                                let album_server_id = album.server_id.clone();
                                                move |_| {
                                                    navigation
                                                        .navigate_to(AppView::AlbumDetailView {
                                                            album_id: album_id.clone(),
                                                            server_id: album_server_id.clone(),
                                                        })
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if album_sections_render.is_empty() {
                    section { class: "mb-8 rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-3 text-sm text-zinc-400",
                        "No album sections enabled."
                    }
                } else {
                    for (section , section_key , items , visible , load_step) in album_sections_render {
                        section { class: "mb-8",
                            div { class: "flex items-center justify-between mb-4",
                                h2 { class: "text-xl font-semibold text-white", "{section.title}" }
                                button {
                                    class: "text-sm text-zinc-400 hover:text-white transition-colors",
                                    onclick: {
                                        let nav = navigation.clone();
                                        move |_| nav.navigate_to(AppView::Albums {})
                                    },
                                    "See all"
                                }
                            }
                            if items.is_empty() {
                                div { class: "rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-4 text-sm text-zinc-400",
                                    "No albums matched this section."
                                }
                            } else {
                                div { class: "overflow-x-auto",
                                    div { class: "flex gap-4 pb-2 min-w-min",
                                        for album in items.iter().take(visible) {
                                            div { class: "w-36 flex-shrink-0",
                                                AlbumCard {
                                                    album: album.clone(),
                                                    onclick: {
                                                        let navigation = navigation.clone();
                                                        let album_id = album.id.clone();
                                                        let album_server_id = album.server_id.clone();
                                                        move |_| {
                                                            navigation
                                                                .navigate_to(AppView::AlbumDetailView {
                                                                    album_id: album_id.clone(),
                                                                    server_id: album_server_id.clone(),
                                                                })
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                        if items.len() > visible {
                                            div { class: "w-36 flex-shrink-0",
                                                LoadMoreStripCard {
                                                    label: format!("Load {} more", load_step.min(items.len().saturating_sub(visible)).max(1)),
                                                    onclick: {
                                                        let mut section_visible_overrides = section_visible_overrides.clone();
                                                        let section_key = section_key.clone();
                                                        let initial_visible = visible;
                                                        let load_step = load_step.max(1);
                                                        move |_| {
                                                            section_visible_overrides
                                                                .with_mut(|map| {
                                                                    let current = map
                                                                        .get(&section_key)
                                                                        .copied()
                                                                        .unwrap_or(initial_visible);
                                                                    map.insert(section_key.clone(), current.saturating_add(load_step));
                                                                });
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if song_sections_render.is_empty() {
                    section { class: "mb-8 rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-3 text-sm text-zinc-400",
                        "No song sections enabled."
                    }
                } else {
                    for (section , section_key , items , visible , load_step) in song_sections_render {
                        section { class: "mb-8",
                            div { class: "flex items-center justify-between mb-4",
                                h2 { class: "text-xl font-semibold text-white", "{section.title}" }
                                button {
                                    class: "text-sm text-zinc-400 hover:text-white transition-colors",
                                    onclick: {
                                        let nav = navigation.clone();
                                        move |_| nav.navigate_to(AppView::SongsView {})
                                    },
                                    "See all"
                                }
                            }
                            if items.is_empty() {
                                div { class: "rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-4 text-sm text-zinc-400",
                                    "No songs matched this section."
                                }
                            } else {
                                div { class: "overflow-x-auto",
                                    div { class: "flex gap-4 pb-2 min-w-min",
                                        for (index , song) in items.iter().take(visible).enumerate() {
                                            div { class: "w-32 flex-shrink-0",
                                                SongCard {
                                                    song: song.clone(),
                                                    onclick: {
                                                        let song = song.clone();
                                                        let songs_for_queue = items.clone();
                                                        move |_| {
                                                            queue.set(songs_for_queue.clone());
                                                            queue_index.set(index);
                                                            now_playing.set(Some(song.clone()));
                                                            is_playing.set(true);
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                        if items.len() > visible {
                                            div { class: "w-32 flex-shrink-0",
                                                LoadMoreStripCard {
                                                    label: format!("Load {} more", load_step.min(items.len().saturating_sub(visible)).max(1)),
                                                    onclick: {
                                                        let mut section_visible_overrides = section_visible_overrides.clone();
                                                        let section_key = section_key.clone();
                                                        let initial_visible = visible;
                                                        let load_step = load_step.max(1);
                                                        move |_| {
                                                            section_visible_overrides
                                                                .with_mut(|map| {
                                                                    let current = map
                                                                        .get(&section_key)
                                                                        .copied()
                                                                        .unwrap_or(initial_visible);
                                                                    map.insert(section_key.clone(), current.saturating_add(load_step));
                                                                });
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if layout_snapshot.quick_picks.enabled {
                    section { class: "mb-8",
                        div { class: "flex items-center justify-between mb-4",
                            h2 { class: "text-xl font-semibold text-white", "Quick Picks" }
                            button {
                                class: "text-sm text-zinc-400 hover:text-white transition-colors",
                                onclick: {
                                    let nav = navigation.clone();
                                    move |_| nav.navigate_to(AppView::SongsView {})
                                },
                                "See all"
                            }
                        }
                        if quick_picks_display.is_empty() {
                            div { class: "rounded-xl border border-zinc-800 bg-zinc-900/50 px-4 py-4 text-sm text-zinc-400",
                                "No quick picks available."
                            }
                        } else {
                            if matches!(layout_snapshot.quick_picks.layout, HomeQuickPicksLayout::Grid) {
                                div {
                                    id: "home-quick-picks-grid",
                                    class: "grid gap-3 w-full",
                                    style: format!(
                                        "grid-template-columns: repeat(auto-fit, minmax({}px, 1fr));",
                                        quick_picks_grid_min_px,
                                    ),
                                    for (index , song) in quick_picks_display.iter().enumerate() {
                                        div { class: "w-full min-w-0",
                                            SongCard {
                                                song: song.clone(),
                                                onclick: {
                                                    let song = song.clone();
                                                    let queue_items = quick_picks_display.clone();
                                                    move |_| {
                                                        queue.set(queue_items.clone());
                                                        queue_index.set(index);
                                                        now_playing.set(Some(song.clone()));
                                                        is_playing.set(true);
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: format!(
                                        "space-y-{}",
                                        match quick_picks_size {
                                            HomeQuickPicksSize::Small => 1,
                                            HomeQuickPicksSize::Medium => 2,
                                            HomeQuickPicksSize::Large => 3,
                                        },
                                    ),
                                    for (index , song) in quick_picks_display.iter().enumerate() {
                                        SongRow {
                                            song: song.clone(),
                                            index: index + 1,
                                            onclick: {
                                                let song = song.clone();
                                                move |_| {
                                                    queue.set(vec![song.clone()]);
                                                    queue_index.set(0);
                                                    now_playing.set(Some(song.clone()));
                                                    is_playing.set(true);
                                                }
                                            },
                                        }
                                    }
                                }
                            }
                            if quick_picks_needs_refresh {
                                div { class: "mt-3 rounded-xl border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-xs text-amber-200 flex flex-wrap items-center justify-between gap-2",
                                    span {
                                        "Showing {quick_pick_items.len()} loaded quick picks. Refresh Home Feed to load more for this layout."
                                    }
                                    button {
                                        class: "inline-flex items-center gap-1 rounded-lg border border-amber-400/50 bg-zinc-900/70 px-2.5 py-1.5 text-xs text-amber-100 hover:text-white hover:border-amber-300 transition-colors",
                                        onclick: on_refresh_home_feed,
                                        Icon {
                                            name: "repeat".to_string(),
                                            class: "w-3.5 h-3.5".to_string(),
                                        }
                                        "Refresh"
                                    }
                                }
                            }
                            if quick_pick_items.len() > quick_picks_visible_limit {
                                div { class: "mt-4 flex justify-center",
                                    button {
                                        class: "inline-flex items-center gap-2 rounded-xl border border-zinc-600 bg-zinc-900/70 px-4 py-2 text-sm text-zinc-200 hover:text-white hover:border-zinc-400 hover:bg-zinc-800/80 transition-colors",
                                        onclick: {
                                            let mut section_visible_overrides = section_visible_overrides.clone();
                                            let quick_picks_section_key = quick_picks_section_key.clone();
                                            let quick_picks_load_step = quick_picks_load_step.max(1);
                                            let initial_visible = quick_picks_visible_limit;
                                            move |_| {
                                                section_visible_overrides
                                                    .with_mut(|map| {
                                                        let current = map
                                                            .get(&quick_picks_section_key)
                                                            .copied()
                                                            .unwrap_or(initial_visible);
                                                        map.insert(
                                                            quick_picks_section_key.clone(),
                                                            current.saturating_add(quick_picks_load_step),
                                                        );
                                                    });
                                            }
                                        },
                                        Icon {
                                            name: "next".to_string(),
                                            class: "w-4 h-4".to_string(),
                                        }
                                        "View More Quick Picks"
                                    }
                                }
                            }
                        }
                    }
                }

                section { class: "pb-6 w-full max-w-2xl mx-auto grid grid-cols-2 gap-3 items-stretch",
                    button {
                        class: "w-full inline-flex items-center justify-center gap-2 rounded-xl border border-emerald-600/60 bg-emerald-500/15 px-3 py-2 text-xs sm:text-sm text-emerald-200 hover:text-white hover:border-emerald-400 transition-colors",
                        onclick: on_open_home_editor,
                        Icon {
                            name: "settings".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Edit Home Screen"
                    }
                    button {
                        class: if is_home_album_loading { "w-full inline-flex items-center justify-center gap-2 rounded-xl border border-zinc-600 bg-zinc-800/60 px-3 py-2 text-xs sm:text-sm text-zinc-300" } else { "w-full inline-flex items-center justify-center gap-2 rounded-xl border border-zinc-600 bg-zinc-900/60 px-3 py-2 text-xs sm:text-sm text-zinc-200 hover:text-white hover:border-zinc-400 hover:bg-zinc-800/70 transition-colors" },
                        onclick: on_refresh_home_feed,
                        disabled: is_home_album_loading,
                        Icon {
                            name: if is_home_album_loading { "loader".to_string() } else { "repeat".to_string() },
                            class: "w-4 h-4".to_string(),
                        }
                        if is_home_album_loading {
                            "Refreshing Home feed..."
                        } else {
                            "Refresh Home Feed"
                        }
                    }
                }

                if show_home_editor() {
                    div { class: "fixed bottom-24 left-1/2 -translate-x-1/2 z-[180]",
                        button {
                            class: "inline-flex items-center gap-2 rounded-xl border border-emerald-500/50 bg-zinc-900/95 px-4 py-2 text-xs text-emerald-200 shadow-xl hover:text-white hover:border-emerald-400 transition-colors",
                            onclick: move |_| {
                                let _ = document::eval(
                                    r#"
(() => {
  const editor = document.getElementById("home-layout-editor");
  if (!editor) return false;
  editor.scrollIntoView({ behavior: "smooth", block: "start" });
  return true;
})();
"#,
                                );
                            },
                            Icon {
                                name: "arrow-down".to_string(),
                                class: "w-3.5 h-3.5".to_string(),
                            }
                            "Editor Open Below — Scroll Down"
                        }
                    }
                }

                if show_home_editor() {
                    section {
                        id: "home-layout-editor",
                        class: "mb-8 rounded-2xl border border-zinc-700/50 bg-gradient-to-br from-zinc-900/80 via-zinc-900/70 to-zinc-950/80 p-6 md:p-8 space-y-8",
                        // Header
                        div { class: "flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4 pb-6 border-b border-zinc-700/30",
                            div { class: "space-y-2",
                                div { class: "flex items-center gap-2",
                                    Icon {
                                        name: "settings".to_string(),
                                        class: "w-5 h-5 text-emerald-400/70".to_string(),
                                    }
                                    h2 { class: "text-2xl font-bold text-white", "Home Layout" }
                                }
                                p { class: "text-sm text-zinc-400 leading-relaxed",
                                    "Customize your feed's appearance, load behavior, and content sections."
                                }
                                p { class: "text-xs text-emerald-300/80",
                                    "Live preview is on while this editor is open."
                                }
                            }
                            div { class: "flex items-center gap-3",
                                button {
                                    class: "px-4 py-2.5 rounded-lg border border-zinc-600 text-zinc-300 hover:text-white hover:bg-zinc-800/40 hover:border-zinc-500 transition-colors text-sm font-medium",
                                    onclick: on_cancel_home_editor,
                                    Icon {
                                        name: "x".to_string(),
                                        class: "w-4 h-4 inline mr-1".to_string(),
                                    }
                                    "Cancel"
                                }
                                button {
                                    class: "px-4 py-2.5 rounded-lg border border-emerald-500/60 bg-gradient-to-r from-emerald-500/20 to-teal-500/10 text-emerald-100 hover:text-white hover:border-emerald-400 hover:bg-emerald-500/25 transition-all text-sm font-medium",
                                    onclick: on_apply_home_editor,
                                    Icon {
                                        name: "check".to_string(),
                                        class: "w-4 h-4 inline mr-1".to_string(),
                                    }
                                    "Apply Changes"
                                }
                            }
                        }

                        // Primary Settings
                        div { class: "grid gap-4 md:grid-cols-2 lg:grid-cols-3",
                            div { class: "lg:col-span-2 rounded-xl border border-zinc-700/40 bg-zinc-900/40 backdrop-blur-sm p-5 space-y-4 hover:border-zinc-600/50 transition-colors",
                                div { class: "flex items-center gap-2 mb-2",
                                    div { class: "w-2 h-2 rounded-full bg-emerald-400/70" }
                                    p { class: "text-sm font-semibold text-white",
                                        "Content Loading"
                                    }
                                }
                                label { class: "space-y-2 block",
                                    p { class: "text-xs uppercase tracking-wider text-zinc-400 font-medium",
                                        "Feed Load Profile"
                                    }
                                    select {
                                        class: "w-full px-3 py-2.5 bg-zinc-800/60 border border-zinc-700/60 rounded-lg text-sm text-white focus:outline-none focus:border-emerald-500/50 transition-colors",
                                        value: layout_draft_snapshot.fetch_profile.as_storage(),
                                        oninput: {
                                            let mut home_layout_draft = home_layout_draft.clone();
                                            move |evt| {
                                                let value = evt.value();
                                                home_layout_draft
                                                    .with_mut(|layout| {
                                                        layout.fetch_profile = HomeFeedLoadProfile::from_storage(&value);
                                                    });
                                            }
                                        },
                                        option { value: "conservative",
                                            "Conservative – Fewer items, faster loading"
                                        }
                                        option { value: "standard", "Standard – Balanced (Default)" }
                                        option { value: "super", "Super – More items, larger layouts" }
                                    }
                                }
                            }
                            div { class: "rounded-xl border border-zinc-700/40 bg-zinc-900/40 backdrop-blur-sm p-5 space-y-4 hover:border-zinc-600/50 transition-colors",
                                div { class: "flex items-center gap-2 mb-2",
                                    div { class: "w-2 h-2 rounded-full bg-emerald-400/70" }
                                    p { class: "text-sm font-semibold text-white",
                                        "Display Mode"
                                    }
                                }
                                label { class: "space-y-2 block",
                                    p { class: "text-xs uppercase tracking-wider text-zinc-400 font-medium",
                                        "Top Strip"
                                    }
                                    select {
                                        class: "w-full px-3 py-2.5 bg-zinc-800/60 border border-zinc-700/60 rounded-lg text-sm text-white focus:outline-none focus:border-emerald-500/50 transition-colors",
                                        value: layout_draft_snapshot.top_strip_mode.as_value(),
                                        oninput: {
                                            let mut home_layout_draft = home_layout_draft.clone();
                                            move |evt| {
                                                let value = evt.value();
                                                home_layout_draft
                                                    .with_mut(|layout| {
                                                        layout.top_strip_mode = HomeTopStripMode::from_value(&value);
                                                    });
                                            }
                                        },
                                        option { value: "quick_play", "Quick Play Cards" }
                                        option { value: "album_highlights", "Album Highlights" }
                                        option { value: "mixed", "Quick Play + Albums" }
                                    }
                                }
                            }
                        }

                        // Album Highlights Settings (Conditional)
                        if matches!(
                            layout_draft_snapshot.top_strip_mode,
                            HomeTopStripMode::AlbumHighlights | HomeTopStripMode::Mixed
                        )
                        {
                            div { class: "rounded-xl border border-zinc-700/40 bg-zinc-900/40 backdrop-blur-sm p-5 space-y-3",
                                p { class: "text-xs uppercase tracking-wider text-zinc-400 font-medium",
                                    "Album Highlights Configuration"
                                }
                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                                    label { class: "space-y-2 block",
                                        p { class: "text-xs text-zinc-300", "Source" }
                                        select {
                                            class: "w-full px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded text-xs text-white focus:outline-none focus:border-emerald-500/50 transition-colors",
                                            value: layout_draft_snapshot.top_album_source.as_value(),
                                            oninput: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |evt| {
                                                    let value = evt.value();
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            layout.top_album_source = HomeAlbumSource::from_value(&value);
                                                        });
                                                }
                                            },
                                            option { value: "recently_added", "Recently Added" }
                                            option { value: "recently_played", "Recently Played" }
                                            option { value: "most_played", "Most Played" }
                                            option { value: "a_to_z", "A-Z" }
                                            option { value: "rating", "Rating" }
                                            option { value: "random", "Random" }
                                        }
                                    }
                                    label { class: "space-y-2 block",
                                        p { class: "text-xs text-zinc-300", "Sort Order" }
                                        select {
                                            class: "w-full px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded text-xs text-white focus:outline-none focus:border-emerald-500/50 transition-colors",
                                            value: layout_draft_snapshot.top_album_direction.as_value(),
                                            oninput: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |evt| {
                                                    let value = evt.value();
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            layout.top_album_direction = HomeSortDirection::from_value(&value);
                                                        });
                                                }
                                            },
                                            option { value: "desc", "Descending" }
                                            option { value: "asc", "Ascending" }
                                        }
                                    }
                                }
                            }
                        }

                        // Quick Play Cards Section
                        if matches!(
                            layout_draft_snapshot.top_strip_mode,
                            HomeTopStripMode::QuickPlay | HomeTopStripMode::Mixed
                        )
                        {
                            div { class: "rounded-xl border border-zinc-700/40 bg-zinc-900/40 backdrop-blur-sm p-5 space-y-4",
                                div { class: "flex items-center justify-between",
                                    div { class: "flex items-center gap-3",
                                        div { class: "w-2 h-2 rounded-full bg-blue-400/70" }
                                        p { class: "text-sm font-semibold text-white",
                                            "Quick Play Cards"
                                        }
                                    }
                                    div { class: "flex items-center gap-2",
                                        span { class: "text-xs text-zinc-400",
                                            if layout_draft_snapshot.quick_play.enabled {
                                                "Active"
                                            } else {
                                                "Inactive"
                                            }
                                        }
                                        div {
                                            class: format!(
                                                "relative inline-flex items-center w-10 h-6 rounded-full transition-colors cursor-pointer {}",
                                                if layout_draft_snapshot.quick_play.enabled {
                                                    "bg-emerald-500/30 border border-emerald-500/50"
                                                } else {
                                                    "bg-zinc-700/40 border border-zinc-600/50"
                                                },
                                            ),
                                            onclick: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |_| {
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            layout.quick_play.enabled = !layout.quick_play.enabled;
                                                        });
                                                }
                                            },
                                            div {
                                                class: format!(
                                                    "inline-block w-5 h-5 transform rounded-full bg-white shadow-lg transition-transform {}",
                                                    if layout_draft_snapshot.quick_play.enabled {
                                                        "translate-x-5"
                                                    } else {
                                                        "translate-x-0"
                                                    },
                                                ),
                                            }
                                        }
                                    }
                                }
                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-3 pt-2",
                                    label { class: "space-y-2 block",
                                        p { class: "text-xs text-zinc-300 font-medium",
                                            "Rows"
                                        }
                                        input {
                                            class: "w-full px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded text-sm text-white focus:outline-none focus:border-blue-500/50 transition-colors",
                                            r#type: "number",
                                            min: "1",
                                            max: "4",
                                            value: format!("{}", layout_draft_snapshot.quick_play.rows),
                                            oninput: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |evt| {
                                                    if let Ok(value) = evt.value().parse::<u8>() {
                                                        home_layout_draft
                                                            .with_mut(|layout| {
                                                                layout.quick_play.rows = value.clamp(1, 4);
                                                            });
                                                    }
                                                }
                                            },
                                        }
                                    }
                                    label { class: "space-y-2 block",
                                        p { class: "text-xs text-zinc-300 font-medium",
                                            "Columns"
                                        }
                                        input {
                                            class: "w-full px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded text-sm text-white focus:outline-none focus:border-blue-500/50 transition-colors",
                                            r#type: "number",
                                            min: "1",
                                            max: "5",
                                            value: format!("{}", layout_draft_snapshot.quick_play.columns),
                                            oninput: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |evt| {
                                                    if let Ok(value) = evt.value().parse::<u8>() {
                                                        home_layout_draft
                                                            .with_mut(|layout| {
                                                                layout.quick_play.columns = value.clamp(1, 5);
                                                            });
                                                    }
                                                }
                                            },
                                        }
                                    }
                                }
                                div { class: "pt-2",
                                    p { class: "text-xs text-zinc-300 font-medium mb-3",
                                        "Visible Actions"
                                    }
                                    div { class: "flex flex-wrap gap-2",
                                        for action in HomeQuickPlayAction::all() {
                                            button {
                                                class: format!(
                                                    "px-3 py-1.5 rounded-lg border text-xs font-medium transition-all {}",
                                                    if layout_draft_snapshot.quick_play.actions.contains(&action) {
                                                        "bg-blue-500/20 border-blue-500/50 text-blue-100 hover:bg-blue-500/30"
                                                    } else {
                                                        "bg-zinc-800/30 border-zinc-700/50 text-zinc-400 hover:bg-zinc-800/50 hover:border-zinc-600/50"
                                                    },
                                                ),
                                                onclick: {
                                                    let mut home_layout_draft = home_layout_draft.clone();
                                                    move |_| {
                                                        home_layout_draft
                                                            .with_mut(|layout| {
                                                                if let Some(index) = layout
                                                                    .quick_play
                                                                    .actions
                                                                    .iter()
                                                                    .position(|item| *item == action)
                                                                {
                                                                    layout.quick_play.actions.remove(index);
                                                                } else {
                                                                    layout.quick_play.actions.push(action);
                                                                }
                                                                if layout.quick_play.actions.is_empty() {
                                                                    layout.quick_play.actions.push(HomeQuickPlayAction::AllSongs);
                                                                }
                                                            });
                                                    }
                                                },
                                                "{quick_play_action_title(action)}"
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Album Sections
                        div { class: "rounded-xl border border-zinc-700/40 bg-zinc-900/40 backdrop-blur-sm p-5 space-y-4",
                            div { class: "flex items-center justify-between",
                                div { class: "flex items-center gap-2",
                                    div { class: "w-2 h-2 rounded-full bg-purple-400/70" }
                                    p { class: "text-sm font-semibold text-white",
                                        "Album Sections"
                                    }
                                    div { class: "text-xs bg-zinc-800/60 text-zinc-300 px-2 py-1 rounded-full",
                                        "{layout_draft_snapshot.album_sections.len()} sections"
                                    }
                                }
                                button {
                                    class: "px-3 py-1.5 rounded-lg border border-purple-500/40 bg-purple-500/10 text-purple-200 hover:bg-purple-500/20 hover:border-purple-500/60 transition-colors text-xs font-medium",
                                    onclick: {
                                        let mut home_layout_draft = home_layout_draft.clone();
                                        move |_| {
                                            home_layout_draft
                                                .with_mut(|layout| {
                                                    layout.album_sections.push(new_album_section());
                                                });
                                        }
                                    },
                                    Icon {
                                        name: "plus".to_string(),
                                        class: "w-3 h-3 inline mr-1".to_string(),
                                    }
                                    "Add Section"
                                }
                            }
                            for (index , section) in layout_draft_snapshot.album_sections.iter().enumerate() {
                                div { class: "mt-3 pt-3 border-t border-zinc-700/30",
                                    div { class: "flex items-center gap-2 mb-3",
                                        input {
                                            class: "flex-1 px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded-lg text-sm text-white focus:outline-none focus:border-purple-500/50 transition-colors font-medium",
                                            placeholder: "Section title",
                                            value: section.title.clone(),
                                            oninput: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |evt| {
                                                    let value = evt.value();
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            if let Some(item) = layout.album_sections.get_mut(index) {
                                                                item.title = value.clone();
                                                            }
                                                        });
                                                }
                                            },
                                        }
                                        div {
                                            class: format!(
                                                "relative inline-flex items-center w-10 h-6 rounded-full transition-colors {}",
                                                if section.enabled {
                                                    "bg-purple-500/30 border border-purple-500/50"
                                                } else {
                                                    "bg-zinc-700/40 border border-zinc-600/50"
                                                },
                                            ),
                                            onclick: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |_| {
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            if let Some(item) = layout.album_sections.get_mut(index) {
                                                                item.enabled = !item.enabled;
                                                            }
                                                        });
                                                }
                                            },
                                            div {
                                                class: format!(
                                                    "inline-block w-5 h-5 transform rounded-full bg-white shadow-lg transition-transform {}",
                                                    if section.enabled { "translate-x-5" } else { "translate-x-0" },
                                                ),
                                            }
                                        }
                                        button {
                                            class: "px-2.5 py-1.5 rounded-lg bg-red-500/10 border border-red-500/30 text-red-300 hover:bg-red-500/20 hover:border-red-500/50 transition-colors text-xs",
                                            onclick: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |_| {
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            if layout.album_sections.len() > 1
                                                                && index < layout.album_sections.len()
                                                            {
                                                                layout.album_sections.remove(index);
                                                            }
                                                        });
                                                }
                                            },
                                            Icon {
                                                name: "trash".to_string(),
                                                class: "w-3 h-3".to_string(),
                                            }
                                        }
                                    }
                                    div {
                                        class: if matches!(section.source, HomeAlbumSource::Rating) {
                                            "grid grid-cols-2 md:grid-cols-3 gap-2"
                                        } else {
                                            "grid grid-cols-2 gap-2"
                                        },
                                        label { class: "space-y-1",
                                            p { class: "text-xs text-zinc-400", "Source" }
                                            select {
                                                class: "w-full px-2 py-1.5 bg-zinc-800/60 border border-zinc-700/60 rounded text-xs text-white focus:outline-none focus:border-purple-500/50 transition-colors",
                                                value: section.source.as_value(),
                                                oninput: {
                                                    let mut home_layout_draft = home_layout_draft.clone();
                                                    move |evt| {
                                                        let value = evt.value();
                                                        home_layout_draft
                                                            .with_mut(|layout| {
                                                                if let Some(item) = layout.album_sections.get_mut(index) {
                                                                    let next_source = HomeAlbumSource::from_value(&value);
                                                                    let previous_default =
                                                                        default_album_section_title(item.source);
                                                                    if should_autoname_section_title(
                                                                        &item.title,
                                                                        previous_default,
                                                                    ) {
                                                                        item.title =
                                                                            default_album_section_title(next_source)
                                                                                .to_string();
                                                                    }
                                                                    item.source = next_source;
                                                                    if !matches!(
                                                                        next_source,
                                                                        HomeAlbumSource::Rating
                                                                    ) {
                                                                        item.min_rating = 0;
                                                                    }
                                                                }
                                                            });
                                                    }
                                                },
                                                option { value: "recently_added", "Recently Added" }
                                                option { value: "recently_played", "Recently Played" }
                                                option { value: "most_played", "Most Played" }
                                                option { value: "a_to_z", "A-Z" }
                                                option { value: "rating", "Rating" }
                                                option { value: "random", "Random" }
                                            }
                                        }
                                        label { class: "space-y-1",
                                            p { class: "text-xs text-zinc-400", "Sort" }
                                            select {
                                                class: "w-full px-2 py-1.5 bg-zinc-800/60 border border-zinc-700/60 rounded text-xs text-white focus:outline-none focus:border-purple-500/50 transition-colors",
                                                value: section.direction.as_value(),
                                                oninput: {
                                                    let mut home_layout_draft = home_layout_draft.clone();
                                                    move |evt| {
                                                        let value = evt.value();
                                                        home_layout_draft
                                                            .with_mut(|layout| {
                                                                if let Some(item) = layout.album_sections.get_mut(index) {
                                                                    item.direction = HomeSortDirection::from_value(&value);
                                                                }
                                                            });
                                                    }
                                                },
                                                option { value: "desc", "↓ Desc" }
                                                option { value: "asc", "↑ Asc" }
                                            }
                                        }
                                        if matches!(section.source, HomeAlbumSource::Rating) {
                                            label { class: "space-y-1",
                                                p { class: "text-xs text-zinc-400", "Min Rating" }
                                                input {
                                                    class: "w-full px-2 py-1.5 bg-zinc-800/60 border border-zinc-700/60 rounded text-xs text-white focus:outline-none focus:border-purple-500/50 transition-colors",
                                                    r#type: "number",
                                                    min: "0",
                                                    max: "5",
                                                    value: format!("{}", section.min_rating),
                                                    oninput: {
                                                        let mut home_layout_draft = home_layout_draft.clone();
                                                        move |evt| {
                                                            if let Ok(value) = evt.value().parse::<u8>() {
                                                                home_layout_draft
                                                                    .with_mut(|layout| {
                                                                        if let Some(item) = layout.album_sections.get_mut(index) {
                                                                            item.min_rating = value.min(5);
                                                                        }
                                                                    });
                                                            }
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                    p { class: "text-[11px] text-zinc-500",
                                        "Visible count and load step are auto-sized from Feed Load Profile and content size."
                                    }
                                }
                            }
                        }

                        // Song Sections
                        div { class: "rounded-xl border border-zinc-700/40 bg-zinc-900/40 backdrop-blur-sm p-5 space-y-4",
                            div { class: "flex items-center justify-between",
                                div { class: "flex items-center gap-2",
                                    div { class: "w-2 h-2 rounded-full bg-rose-400/70" }
                                    p { class: "text-sm font-semibold text-white",
                                        "Song Sections"
                                    }
                                    div { class: "text-xs bg-zinc-800/60 text-zinc-300 px-2 py-1 rounded-full",
                                        "{layout_draft_snapshot.song_sections.len()} sections"
                                    }
                                }
                                button {
                                    class: "px-3 py-1.5 rounded-lg border border-rose-500/40 bg-rose-500/10 text-rose-200 hover:bg-rose-500/20 hover:border-rose-500/60 transition-colors text-xs font-medium",
                                    onclick: {
                                        let mut home_layout_draft = home_layout_draft.clone();
                                        move |_| {
                                            home_layout_draft
                                                .with_mut(|layout| {
                                                    layout.song_sections.push(new_song_section());
                                                });
                                        }
                                    },
                                    Icon {
                                        name: "plus".to_string(),
                                        class: "w-3 h-3 inline mr-1".to_string(),
                                    }
                                    "Add Section"
                                }
                            }
                            for (index , section) in layout_draft_snapshot.song_sections.iter().enumerate() {
                                div { class: "mt-3 pt-3 border-t border-zinc-700/30",
                                    div { class: "flex items-center gap-2 mb-3",
                                        input {
                                            class: "flex-1 px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded-lg text-sm text-white focus:outline-none focus:border-rose-500/50 transition-colors font-medium",
                                            placeholder: "Section title",
                                            value: section.title.clone(),
                                            oninput: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |evt| {
                                                    let value = evt.value();
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            if let Some(item) = layout.song_sections.get_mut(index) {
                                                                item.title = value.clone();
                                                            }
                                                        });
                                                }
                                            },
                                        }
                                        div {
                                            class: format!(
                                                "relative inline-flex items-center w-10 h-6 rounded-full transition-colors {}",
                                                if section.enabled {
                                                    "bg-rose-500/30 border border-rose-500/50"
                                                } else {
                                                    "bg-zinc-700/40 border border-zinc-600/50"
                                                },
                                            ),
                                            onclick: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |_| {
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            if let Some(item) = layout.song_sections.get_mut(index) {
                                                                item.enabled = !item.enabled;
                                                            }
                                                        });
                                                }
                                            },
                                            div {
                                                class: format!(
                                                    "inline-block w-5 h-5 transform rounded-full bg-white shadow-lg transition-transform {}",
                                                    if section.enabled { "translate-x-5" } else { "translate-x-0" },
                                                ),
                                            }
                                        }
                                        button {
                                            class: "px-2.5 py-1.5 rounded-lg bg-red-500/10 border border-red-500/30 text-red-300 hover:bg-red-500/20 hover:border-red-500/50 transition-colors text-xs",
                                            onclick: {
                                                let mut home_layout_draft = home_layout_draft.clone();
                                                move |_| {
                                                    home_layout_draft
                                                        .with_mut(|layout| {
                                                            if layout.song_sections.len() > 1
                                                                && index < layout.song_sections.len()
                                                            {
                                                                layout.song_sections.remove(index);
                                                            }
                                                        });
                                                }
                                            },
                                            Icon {
                                                name: "trash".to_string(),
                                                class: "w-3 h-3".to_string(),
                                            }
                                        }
                                    }
                                    div {
                                        class: if matches!(section.source, HomeSongSource::Rating) {
                                            "grid grid-cols-2 md:grid-cols-3 gap-2"
                                        } else {
                                            "grid grid-cols-2 gap-2"
                                        },
                                        label { class: "space-y-1",
                                            p { class: "text-xs text-zinc-400", "Source" }
                                            select {
                                                class: "w-full px-2 py-1.5 bg-zinc-800/60 border border-zinc-700/60 rounded text-xs text-white focus:outline-none focus:border-rose-500/50 transition-colors",
                                                value: section.source.as_value(),
                                                oninput: {
                                                    let mut home_layout_draft = home_layout_draft.clone();
                                                    move |evt| {
                                                        let value = evt.value();
                                                        home_layout_draft
                                                            .with_mut(|layout| {
                                                                if let Some(item) = layout.song_sections.get_mut(index) {
                                                                    let next_source = HomeSongSource::from_value(&value);
                                                                    let previous_default =
                                                                        default_song_section_title(item.source);
                                                                    if should_autoname_section_title(
                                                                        &item.title,
                                                                        previous_default,
                                                                    ) {
                                                                        item.title =
                                                                            default_song_section_title(next_source)
                                                                                .to_string();
                                                                    }
                                                                    item.source = next_source;
                                                                    if !matches!(
                                                                        next_source,
                                                                        HomeSongSource::Rating
                                                                    ) {
                                                                        item.min_rating = 0;
                                                                    }
                                                                }
                                                            });
                                                    }
                                                },
                                                option { value: "most_played", "Most Played" }
                                                option { value: "recently_played", "Recently Played" }
                                                option { value: "random", "Random" }
                                                option { value: "a_to_z", "A-Z" }
                                                option { value: "rating", "Rating" }
                                                option { value: "quick_picks", "Quick Picks" }
                                            }
                                        }
                                        label { class: "space-y-1",
                                            p { class: "text-xs text-zinc-400", "Sort" }
                                            select {
                                                class: "w-full px-2 py-1.5 bg-zinc-800/60 border border-zinc-700/60 rounded text-xs text-white focus:outline-none focus:border-rose-500/50 transition-colors",
                                                value: section.direction.as_value(),
                                                oninput: {
                                                    let mut home_layout_draft = home_layout_draft.clone();
                                                    move |evt| {
                                                        let value = evt.value();
                                                        home_layout_draft
                                                            .with_mut(|layout| {
                                                                if let Some(item) = layout.song_sections.get_mut(index) {
                                                                    item.direction = HomeSortDirection::from_value(&value);
                                                                }
                                                            });
                                                    }
                                                },
                                                option { value: "desc", "↓ Desc" }
                                                option { value: "asc", "↑ Asc" }
                                            }
                                        }
                                        if matches!(section.source, HomeSongSource::Rating) {
                                            label { class: "space-y-1",
                                                p { class: "text-xs text-zinc-400", "Min Rating" }
                                                input {
                                                    class: "w-full px-2 py-1.5 bg-zinc-800/60 border border-zinc-700/60 rounded text-xs text-white focus:outline-none focus:border-rose-500/50 transition-colors",
                                                    r#type: "number",
                                                    min: "0",
                                                    max: "5",
                                                    value: format!("{}", section.min_rating),
                                                    oninput: {
                                                        let mut home_layout_draft = home_layout_draft.clone();
                                                        move |evt| {
                                                            if let Ok(value) = evt.value().parse::<u8>() {
                                                                home_layout_draft
                                                                    .with_mut(|layout| {
                                                                        if let Some(item) = layout.song_sections.get_mut(index) {
                                                                            item.min_rating = value.min(5);
                                                                        }
                                                                    });
                                                            }
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                    p { class: "text-[11px] text-zinc-500",
                                        "Visible count and load step are auto-sized from Feed Load Profile and content size."
                                    }
                                }
                            }
                        }

                        // Quick Picks Section
                        div { class: "rounded-xl border border-zinc-700/40 bg-zinc-900/40 backdrop-blur-sm p-5 space-y-4",
                            div { class: "flex items-center justify-between",
                                div { class: "flex items-center gap-3",
                                    div { class: "w-2 h-2 rounded-full bg-orange-400/70" }
                                    p { class: "text-sm font-semibold text-white",
                                        "Quick Picks Section"
                                    }
                                    span { class: "text-xs text-zinc-400",
                                        if layout_draft_snapshot.quick_picks.enabled {
                                            "Visible"
                                        } else {
                                            "Hidden"
                                        }
                                    }
                                }
                                div {
                                    class: format!(
                                        "relative inline-flex items-center w-10 h-6 rounded-full transition-colors {}",
                                        if layout_draft_snapshot.quick_picks.enabled {
                                            "bg-orange-500/30 border border-orange-500/50"
                                        } else {
                                            "bg-zinc-700/40 border border-zinc-600/50"
                                        },
                                    ),
                                    onclick: {
                                        let mut home_layout_draft = home_layout_draft.clone();
                                        move |_| {
                                            home_layout_draft
                                                .with_mut(|layout| {
                                                    layout.quick_picks.enabled = !layout.quick_picks.enabled;
                                                });
                                        }
                                    },
                                    div {
                                        class: format!(
                                            "inline-block w-5 h-5 transform rounded-full bg-white shadow-lg transition-transform {}",
                                            if layout_draft_snapshot.quick_picks.enabled {
                                                "translate-x-5"
                                            } else {
                                                "translate-x-0"
                                            },
                                        ),
                                    }
                                }
                            }
                            div { class: "grid grid-cols-1 md:grid-cols-2 gap-3 pt-2",
                                label { class: "space-y-2 block",
                                    p { class: "text-xs text-zinc-300 font-medium",
                                        "Layout Mode"
                                    }
                                    select {
                                        class: "w-full px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded text-sm text-white focus:outline-none focus:border-orange-500/50 transition-colors",
                                        value: layout_draft_snapshot.quick_picks.layout.as_value(),
                                        oninput: {
                                            let mut home_layout_draft = home_layout_draft.clone();
                                            move |evt| {
                                                let value = evt.value();
                                                home_layout_draft
                                                    .with_mut(|layout| {
                                                        layout.quick_picks.layout = HomeQuickPicksLayout::from_value(&value);
                                                        layout.quick_picks.visible_count =
                                                            normalize_quick_picks_requested_count(
                                                                layout.quick_picks.visible_count
                                                                    as usize,
                                                            )
                                                            as u8;
                                                    });
                                            }
                                        },
                                        option { value: "list", "List View" }
                                        option { value: "grid", "Grid View" }
                                    }
                                }
                                label { class: "space-y-2 block",
                                    p { class: "text-xs text-zinc-300 font-medium",
                                        "Card Size"
                                    }
                                    select {
                                        class: "w-full px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded text-sm text-white focus:outline-none focus:border-orange-500/50 transition-colors",
                                        value: layout_draft_snapshot.quick_picks.size.as_value(),
                                        oninput: {
                                            let mut home_layout_draft = home_layout_draft.clone();
                                            move |evt| {
                                                let value = evt.value();
                                                home_layout_draft
                                                    .with_mut(|layout| {
                                                        layout.quick_picks.size = HomeQuickPicksSize::from_value(&value);
                                                    });
                                            }
                                        },
                                        option { value: "small", "Small" }
                                        option { value: "medium", "Medium" }
                                        option { value: "large", "Large" }
                                    }
                                }
                                label { class: "space-y-2 block",
                                    p { class: "text-xs text-zinc-300 font-medium",
                                        "Visible Amount"
                                    }
                                    select {
                                        class: "w-full px-3 py-2 bg-zinc-800/60 border border-zinc-700/60 rounded text-sm text-white focus:outline-none focus:border-orange-500/50 transition-colors",
                                        value: quick_picks_visible_amount_from_count(
                                            layout_draft_snapshot.quick_picks.visible_count as usize,
                                        )
                                        .as_value(),
                                        oninput: {
                                            let mut home_layout_draft = home_layout_draft.clone();
                                            move |evt| {
                                                let amount =
                                                    QuickPicksVisibleAmount::from_value(&evt.value());
                                                let target = quick_picks_visible_count_for_amount(amount);
                                                home_layout_draft
                                                    .with_mut(|layout| {
                                                        layout.quick_picks.visible_count =
                                                            normalize_quick_picks_requested_count(target)
                                                                as u8;
                                                    });
                                            }
                                        },
                                        option { value: "small", "Small (14)" }
                                        option { value: "medium", "Medium (~25)" }
                                        option { value: "large", "Large (40)" }
                                    }
                                }
                            }
                        }

                        // Footer Actions
                        div { class: "flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 pt-4 border-t border-zinc-700/30 mt-4",
                            button {
                                class: "px-4 py-2.5 rounded-lg border border-zinc-600 text-zinc-300 hover:text-white hover:bg-zinc-800/40 hover:border-zinc-500 transition-colors text-sm font-medium",
                                onclick: on_reset_home_editor_defaults,
                                Icon {
                                    name: "refresh-cw".to_string(),
                                    class: "w-4 h-4 inline mr-2".to_string(),
                                }
                                "Reset To Defaults"
                            }
                            p { class: "text-xs text-zinc-400 sm:text-right",
                                "Restore the default out-of-box Home layout configuration"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn QuickPlayCard(
    title: String,
    gradient: String,
    icon: String,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            class: "flex items-center gap-3 p-4 rounded-xl bg-zinc-800/50 hover:bg-zinc-800 transition-colors text-left group min-w-0",
            onclick: move |e| onclick.call(e),
            div { class: "w-12 h-12 rounded-lg bg-gradient-to-br {gradient} flex items-center justify-center shadow-lg flex-shrink-0",
                Icon { name: icon, class: "w-5 h-5 text-white".to_string() }
            }
            span { class: "min-w-0 flex-1 font-medium text-sm sm:text-base text-white truncate group-hover:text-emerald-400 transition-colors",
                "{title}"
            }
        }
    }
}

#[component]
fn AlbumHighlightCard(album: Album, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let cover_url = servers()
        .iter()
        .find(|server| server.id == album.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            album
                .cover_art
                .as_ref()
                .map(|cover_art| client.get_cover_art_url(cover_art, 120))
        });
    let album_name = album.name.clone();
    let album_artist = album.artist.clone();

    rsx! {
        button {
            class: "flex items-center gap-3 p-4 rounded-xl bg-zinc-800/50 hover:bg-zinc-800 transition-colors text-left group min-w-0",
            onclick: move |e| onclick.call(e),
            div { class: "w-12 h-12 rounded-lg overflow-hidden bg-zinc-800 flex-shrink-0 border border-zinc-700/60",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                Icon {
                                    name: "album".to_string(),
                                    class: "w-5 h-5 text-zinc-500".to_string(),
                                }
                            }
                        },
                    }
                }
            }
            div { class: "min-w-0 flex-1",
                p { class: "font-medium text-sm sm:text-base text-white truncate group-hover:text-emerald-400 transition-colors",
                    "{album_name}"
                }
                p { class: "text-[11px] sm:text-xs text-zinc-400 truncate", "{album_artist}" }
            }
        }
    }
}

#[component]
fn LoadMoreStripCard(label: String, onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        button {
            class: "flex-shrink-0 w-full min-w-0 aspect-square rounded-xl border border-dashed border-zinc-700 bg-zinc-900/30 hover:border-emerald-500/70 hover:bg-emerald-500/10 text-zinc-300 hover:text-white transition-colors flex flex-col items-center justify-center gap-2",
            style: "width: 100% !important;",
            onclick: move |evt| onclick.call(evt),
            Icon { name: "next".to_string(), class: "w-5 h-5".to_string() }
            span { class: "text-xs font-medium text-center px-2", "{label}" }
        }
    }
}

#[component]
fn SongCard(song: Song, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let current_rating = use_signal(move || song.user_rating.unwrap_or(0).min(5));
    let is_favorited = use_signal(|| song.starred.is_some());
    let mut show_context_menu = use_signal(|| false);
    let download_busy = use_signal(|| false);
    let initially_downloaded = is_song_downloaded(&song);
    let downloaded = use_signal(move || initially_downloaded);
    let mut menu_x = use_signal(|| 0f64);
    let mut menu_y = use_signal(|| 0f64);

    let cover_url = servers()
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 120))
        });

    let make_on_open_menu = {
        let add_menu = add_menu.clone();
        let song = song.clone();
        let show_context_menu = show_context_menu.clone();
        move || {
            let mut add_menu = add_menu.clone();
            let song = song.clone();
            let mut show_context_menu = show_context_menu.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_context_menu.set(false);
                add_menu.open(AddIntent::from_song(song.clone()));
            }
        }
    };

    let make_on_set_rating = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let show_context_menu = show_context_menu.clone();
        move |new_rating: u32| {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut current_rating = current_rating.clone();
            let mut show_context_menu = show_context_menu.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_context_menu.set(false);
                current_rating.set(new_rating.min(5));
                let servers = servers.clone();
                let song_id = song_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let _ = client.set_rating(&song_id, new_rating.min(5)).await;
                    }
                });
            }
        }
    };

    let on_toggle_favorite = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let mut now_playing = now_playing.clone();
        let mut queue = queue.clone();
        let mut is_favorited = is_favorited.clone();
        let mut show_context_menu = show_context_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            let should_star = !is_favorited();
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            spawn(async move {
                let servers_snapshot = servers();
                if let Some(server) = servers_snapshot.iter().find(|s| s.id == server_id) {
                    let client = NavidromeClient::new(server.clone());
                    let result = if should_star {
                        client.star(&song_id, "song").await
                    } else {
                        client.unstar(&song_id, "song").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                        now_playing.with_mut(|current| {
                            if let Some(ref mut s) = current {
                                if s.id == song_id {
                                    s.starred = if should_star {
                                        Some("local".to_string())
                                    } else {
                                        None
                                    };
                                }
                            }
                        });
                        queue.with_mut(|items| {
                            for s in items.iter_mut() {
                                if s.id == song_id {
                                    s.starred = if should_star {
                                        Some("local".to_string())
                                    } else {
                                        None
                                    };
                                }
                            }
                        });
                    }
                }
            });
        }
    };
    let make_on_toggle_favorite = {
        let on_toggle_favorite = on_toggle_favorite.clone();
        move || {
            let mut on_toggle_favorite = on_toggle_favorite.clone();
            move |evt: MouseEvent| on_toggle_favorite(evt)
        }
    };

    let on_download_song = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let song = song.clone();
        let mut download_busy = download_busy.clone();
        let mut downloaded = downloaded.clone();
        let mut show_context_menu = show_context_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            if download_busy() || downloaded() {
                return;
            }
            let servers_snapshot = servers();
            if servers_snapshot.is_empty() {
                return;
            }
            let mut settings_snapshot = app_settings();
            settings_snapshot.downloads_enabled = true;
            download_busy.set(true);
            let song = song.clone();
            spawn(async move {
                if prefetch_song_audio(&song, &servers_snapshot, &settings_snapshot)
                    .await
                    .is_ok()
                {
                    downloaded.set(true);
                }
                download_busy.set(false);
            });
        }
    };

    let make_on_download_song = {
        let on_download_song = on_download_song.clone();
        move || {
            let mut on_download_song = on_download_song.clone();
            move |evt: MouseEvent| on_download_song(evt)
        }
    };

    rsx! {
        div {
            class: "rs-carousel-item relative group text-left cursor-pointer flex-shrink-0 w-full min-w-0",
            style: "width: 100% !important;",
            onclick: move |e| {
                show_context_menu.set(false);
                onclick.call(e);
            },
            // Cover
            div { class: "rs-album-art aspect-square rounded-xl bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                Icon { name: "music".to_string(), class: "w-8 h-8 text-zinc-500".to_string() }
                            }
                        },
                    }
                }
                // Play overlay (pointer-events-none so the button above is always clickable)
                button {
                    class: "absolute top-2 right-2 p-1.5 rounded-full bg-zinc-950/80 text-zinc-200 hover:text-white hover:bg-emerald-500 hover:scale-105 hover:-translate-y-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-zinc-300/60 transition-all opacity-0 group-hover:opacity-100 z-10",
                    aria_label: "Song options",
                    title: "More options",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let coords = evt.client_coordinates();
                        menu_x.set(coords.x);
                        menu_y.set(coords.y);
                        show_context_menu.set(!show_context_menu());
                    },
                    Icon {
                        name: "more-horizontal".to_string(),
                        class: "w-3.5 h-3.5".to_string(),
                    }
                }
                div { class: "absolute inset-0 bg-black/40 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none" }
                div { class: "absolute inset-0 flex items-center justify-center pointer-events-none",
                    div { class: "w-10 h-10 rounded-full bg-emerald-500/95 text-white transition-all duration-200 ease-out opacity-100 md:opacity-0 md:group-hover:opacity-100 z-10 shadow-xl pointer-events-none flex items-center justify-center group-hover:scale-105 group-hover:-translate-y-0.5",
                        Icon {
                            name: "play".to_string(),
                            class: "w-5 h-5 text-white ml-0.5".to_string(),
                        }
                    }
                }
            }
            // Song info
            div { class: "flex items-center justify-between gap-2 min-w-0",
                div { class: "flex flex-col min-w-0 flex-1",
                    p { class: "font-medium text-white text-sm truncate group-hover:text-emerald-400 transition-colors min-w-0",
                        "{song.title}"
                    }
                    div { class: "mt-1 max-w-full inline-flex items-center gap-1 text-xs text-zinc-400",
                        ArtistNameLinks {
                            artist_text: song.artist.clone().unwrap_or_default(),
                            server_id: song.server_id.clone(),
                            fallback_artist_id: song.artist_id.clone(),
                            container_class: "inline-flex max-w-full min-w-0 items-center gap-1".to_string(),
                            button_class: "inline-flex max-w-fit truncate text-left hover:text-emerald-400 transition-colors"
                                .to_string(),
                            separator_class: "text-zinc-500".to_string(),
                        }
                        if downloaded() {
                            Icon {
                                name: "download".to_string(),
                                class: "w-3 h-3 text-emerald-400 flex-shrink-0".to_string(),
                            }
                        }
                    }
                }
                div { class: "flex items-center gap-1 flex-shrink-0 -mr-1",
                    button {
                        class: if is_favorited() { "p-1 rounded-lg text-emerald-400 hover:text-emerald-300 hover:bg-emerald-500/10 transition-colors" } else { "p-1 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors" },
                        aria_label: if is_favorited() { "Unfavorite" } else { "Favorite" },
                        onclick: make_on_toggle_favorite(),
                        Icon {
                            name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                            class: "w-3.5 h-3.5".to_string(),
                        }
                    }
                }
            }
            // Context menu
            if show_context_menu() {
                div {
                    class: "fixed inset-0 z-[9998]",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        show_context_menu.set(false);
                    },
                }
                div {
                    class: "fixed z-[9999] w-44 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                    style: anchored_menu_style(menu_x(), menu_y(), 176.0, 320.0),
                    onclick: move |evt: MouseEvent| evt.stop_propagation(),
                    button {
                        class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                        onclick: make_on_toggle_favorite(),
                        Icon {
                            name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                            class: if is_favorited() { "w-4 h-4 text-emerald-400".to_string() } else { "w-4 h-4".to_string() },
                        }
                        if is_favorited() {
                            "Unfavorite"
                        } else {
                            "Favorite"
                        }
                    }
                    if downloaded() {
                        div { class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10",
                            Icon {
                                name: "check".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                            "Downloaded"
                        }
                    } else {
                        button {
                            class: if download_busy() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-500 cursor-not-allowed" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                            disabled: download_busy(),
                            onclick: make_on_download_song(),
                            Icon {
                                name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                            if download_busy() {
                                "Downloading..."
                            } else {
                                "Download"
                            }
                        }
                    }
                    div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500",
                        "Rating"
                    }
                    div { class: "flex items-center gap-1 px-2 pb-1",
                        for i in 1u32..=5u32 {
                            button {
                                class: "p-1 rounded text-amber-400 hover:text-amber-300 transition-colors",
                                onclick: make_on_set_rating(i),
                                Icon {
                                    name: if i <= current_rating() { "star-filled".to_string() } else { "star".to_string() },
                                    class: "w-3.5 h-3.5".to_string(),
                                }
                            }
                        }
                    }
                    button {
                        class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                        onclick: make_on_open_menu(),
                        Icon {
                            name: "plus".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Add to..."
                    }
                    div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500",
                        "Length"
                    }
                    p { class: "px-2.5 pb-2 text-xs text-zinc-300",
                        "{format_duration(song.duration)}"
                    }
                }
            }
        }
    }
}

#[component]
pub fn AlbumCard(album: Album, onclick: EventHandler<MouseEvent>) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let now_playing = use_context::<Signal<Option<Song>>>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let queue_index = use_context::<Signal<usize>>();
    let is_playing = use_context::<crate::components::IsPlayingSignal>().0;
    let shuffle_enabled = use_context::<crate::components::ShuffleEnabledSignal>().0;
    let is_favorited = use_signal(|| album.starred.is_some());
    let album_rating = use_signal(|| album.user_rating.unwrap_or(0).min(5));
    let mut show_context_menu = use_signal(|| false);
    let download_busy = use_signal(|| false);
    let downloaded = use_signal(|| is_album_downloaded(&album.server_id, &album.id));
    let mut menu_x = use_signal(|| 0f64);
    let mut menu_y = use_signal(|| 0f64);

    let cover_url = servers()
        .iter()
        .find(|s| s.id == album.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            album
                .cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 300))
        });

    let on_play_album = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let album = album.clone();
        let mut show_context_menu = show_context_menu.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let now_playing = now_playing.clone();
        let is_playing = is_playing.clone();
        let shuffle_enabled = shuffle_enabled.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            let servers_snapshot = servers();
            let Some(server) = servers_snapshot
                .iter()
                .find(|server| server.id == album.server_id)
                .cloned()
            else {
                return;
            };
            let settings_snapshot = app_settings();
            let album_meta = album.clone();
            let source_id = format!("{}::{}", album_meta.server_id, album_meta.id);
            let mut queue = queue.clone();
            let mut queue_index = queue_index.clone();
            let mut now_playing = now_playing.clone();
            let mut is_playing = is_playing.clone();
            let shuffle_enabled = shuffle_enabled.clone();
            spawn(async move {
                let client = NavidromeClient::new(server);
                if let Ok((_, songs)) = client.get_album(&album_meta.id).await {
                    let playable = if settings_snapshot.offline_mode {
                        songs
                            .into_iter()
                            .filter(|song| is_song_downloaded(song))
                            .collect::<Vec<_>>()
                    } else {
                        songs
                    };
                    if playable.is_empty() {
                        return;
                    }
                    let playable =
                        assign_collection_queue_meta(playable, QueueSourceKind::Album, source_id);
                    queue.set(playable.clone());
                    queue_index.set(0);
                    now_playing.set(Some(playable[0].clone()));
                    is_playing.set(true);
                    if shuffle_enabled() {
                        let _ = apply_collection_shuffle_mode(
                            queue.clone(),
                            queue_index.clone(),
                            now_playing.clone(),
                            true,
                        );
                    }
                }
            });
        }
    };

    let on_toggle_shuffle = {
        let mut shuffle_enabled = shuffle_enabled.clone();
        let queue = queue.clone();
        let queue_index = queue_index.clone();
        let now_playing = now_playing.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            let next = !shuffle_enabled();
            shuffle_enabled.set(next);
            let _ = apply_collection_shuffle_mode(
                queue.clone(),
                queue_index.clone(),
                now_playing.clone(),
                next,
            );
        }
    };

    let on_open_menu_for_context = {
        let mut add_menu = add_menu.clone();
        let album = album.clone();
        let mut show_context_menu = show_context_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            add_menu.open(AddIntent::from_album(&album));
        }
    };

    let on_toggle_favorite = {
        let servers = servers.clone();
        let album_id = album.id.clone();
        let server_id = album.server_id.clone();
        let mut is_favorited = is_favorited.clone();
        let mut show_context_menu = show_context_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            let should_star = !is_favorited();
            let servers = servers.clone();
            let album_id = album_id.clone();
            let server_id = server_id.clone();
            spawn(async move {
                let servers_snapshot = servers();
                if let Some(server) = servers_snapshot.iter().find(|s| s.id == server_id) {
                    let client = NavidromeClient::new(server.clone());
                    let result = if should_star {
                        client.star(&album_id, "album").await
                    } else {
                        client.unstar(&album_id, "album").await
                    };
                    if result.is_ok() {
                        is_favorited.set(should_star);
                    }
                }
            });
        }
    };
    let make_on_toggle_favorite = {
        let on_toggle_favorite = on_toggle_favorite.clone();
        move || {
            let mut on_toggle_favorite = on_toggle_favorite.clone();
            move |evt: MouseEvent| on_toggle_favorite(evt)
        }
    };

    let make_on_set_album_rating = {
        let servers = servers.clone();
        let album_id = album.id.clone();
        let server_id = album.server_id.clone();
        let album_rating = album_rating.clone();
        move |new_rating: u32| {
            let servers = servers.clone();
            let album_id = album_id.clone();
            let server_id = server_id.clone();
            let mut album_rating = album_rating.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                let normalized = new_rating.min(5);
                album_rating.set(normalized);
                let servers = servers.clone();
                let album_id = album_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let _ = client.set_rating(&album_id, normalized).await;
                    }
                });
            }
        }
    };

    let album_artist_names = parse_artist_names(&album.artist);
    let direct_album_artist_id = if album_artist_names.len() == 1 {
        album.artist_id.clone()
    } else {
        None
    };
    let make_on_view_artist_from_menu_named = {
        let servers = servers.clone();
        let navigation = navigation.clone();
        let server_id = album.server_id.clone();
        let show_context_menu = show_context_menu.clone();
        let direct_album_artist_id = direct_album_artist_id.clone();
        move |artist_name: String| {
            let servers = servers.clone();
            let navigation = navigation.clone();
            let server_id = server_id.clone();
            let mut show_context_menu = show_context_menu.clone();
            let direct_album_artist_id = direct_album_artist_id.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_context_menu.set(false);
                if let Some(artist_id) = direct_album_artist_id.clone() {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id,
                        server_id: server_id.clone(),
                    });
                    return;
                }
                let server = servers().iter().find(|s| s.id == server_id).cloned();
                let Some(server) = server else {
                    return;
                };
                let navigation = navigation.clone();
                let server_id = server_id.clone();
                let artist_name = artist_name.clone();
                spawn(async move {
                    if let Some(artist_id) = resolve_artist_id_for_name(server, artist_name).await {
                        navigation.navigate_to(AppView::ArtistDetailView {
                            artist_id,
                            server_id,
                        });
                    }
                });
            }
        }
    };

    let on_view_album_from_menu = {
        let navigation = navigation.clone();
        let album_id = album.id.clone();
        let server_id = album.server_id.clone();
        let mut show_context_menu = show_context_menu.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            navigation.navigate_to(AppView::AlbumDetailView {
                album_id: album_id.clone(),
                server_id: server_id.clone(),
            });
        }
    };

    let on_download_album = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let album = album.clone();
        let mut show_context_menu = show_context_menu.clone();
        let mut download_busy = download_busy.clone();
        let mut downloaded = downloaded.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            show_context_menu.set(false);
            if download_busy() || downloaded() {
                return;
            }
            let servers_snapshot = servers();
            let Some(server) = servers_snapshot
                .iter()
                .find(|server| server.id == album.server_id)
                .cloned()
            else {
                return;
            };
            let settings_snapshot = app_settings();
            let album_meta = album.clone();
            download_busy.set(true);
            spawn(async move {
                let client = NavidromeClient::new(server);
                if let Ok((_, songs)) = client.get_album(&album_meta.id).await {
                    let report =
                        download_songs_batch(&songs, &servers_snapshot, &settings_snapshot).await;
                    if report.downloaded > 0 || report.skipped > 0 {
                        mark_collection_downloaded(
                            "album",
                            &album_meta.server_id,
                            &album_meta.id,
                            &album_meta.name,
                            songs.len(),
                        );
                        sync_downloaded_collection_members(
                            "album",
                            &album_meta.server_id,
                            &album_meta.id,
                            &songs,
                        );
                        downloaded.set(true);
                    }
                }
                download_busy.set(false);
            });
        }
    };
    let make_on_download_album = {
        let on_download_album = on_download_album.clone();
        move || {
            let mut on_download_album = on_download_album.clone();
            move |evt: MouseEvent| on_download_album(evt)
        }
    };

    rsx! {
        div {
            class: "rs-album-card relative group text-left cursor-pointer w-full",
            onclick: move |e| {
                show_context_menu.set(false);
                onclick.call(e);
            },
            // Album cover
            div { class: "rs-album-art aspect-square rounded-xl bg-zinc-800 mb-3 overflow-hidden relative shadow-lg group-hover:shadow-xl transition-shadow",
                {
                    match cover_url {
                        Some(url) => rsx! {
                            img { class: "w-full h-full object-cover", src: "{url}" }
                        },
                        None => rsx! {
                            div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                Icon {
                                    name: "album".to_string(),
                                    class: "w-12 h-12 text-zinc-500".to_string(),
                                }
                            }
                        },
                    }
                }
                div { class: "absolute inset-0 bg-black/20 opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none" }
                div { class: "absolute inset-0 flex items-center justify-center pointer-events-none",
                    button {
                        class: "p-3 rounded-full bg-emerald-500/95 text-white hover:bg-emerald-400 hover:scale-105 hover:-translate-y-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-emerald-300/70 transition-all opacity-100 md:opacity-0 md:group-hover:opacity-100 z-10 shadow-xl pointer-events-auto",
                        aria_label: "Play album",
                        title: "Play album",
                        onclick: on_play_album,
                        Icon {
                            name: "play".to_string(),
                            class: "w-5 h-5 ml-0.5".to_string(),
                        }
                    }
                }
                button {
                    class: "absolute top-3 right-3 p-2 rounded-full bg-zinc-950/85 text-zinc-200 hover:text-white hover:bg-zinc-800 hover:scale-105 hover:-translate-y-0.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-zinc-300/60 transition-all opacity-100 md:opacity-0 md:group-hover:opacity-100 z-10",
                    aria_label: "Album options",
                    title: "More options",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        let coords = evt.client_coordinates();
                        menu_x.set(coords.x);
                        menu_y.set(coords.y);
                        show_context_menu.set(!show_context_menu());
                    },
                    Icon {
                        name: "more-horizontal".to_string(),
                        class: "w-4 h-4".to_string(),
                    }
                }
            }
            // Album info
            div { class: "flex items-center gap-2 w-full min-w-0",
                p {
                    class: "font-medium text-white text-sm group-hover:text-emerald-400 transition-colors truncate max-w-full min-w-0",
                    title: "{album.name}",
                    "{album.name}"
                }
                button {
                    class: if is_favorited() { "flex-shrink-0 p-1 rounded-lg text-emerald-400 hover:text-emerald-300 hover:bg-emerald-500/10 transition-colors" } else { "flex-shrink-0 p-1 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 transition-colors" },
                    aria_label: if is_favorited() { "Unfavorite album" } else { "Favorite album" },
                    onclick: make_on_toggle_favorite(),
                    Icon {
                        name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                        class: "w-3.5 h-3.5".to_string(),
                    }
                }
            }
            div {
                class: "max-w-full inline-flex items-center gap-1 text-xs text-zinc-400 truncate",
                title: "{album.artist}",
                if downloaded() {
                    Icon {
                        name: "download".to_string(),
                        class: "w-3 h-3 text-emerald-400 flex-shrink-0".to_string(),
                    }
                }
                ArtistNameLinks {
                    artist_text: album.artist.clone(),
                    server_id: album.server_id.clone(),
                    fallback_artist_id: album.artist_id.clone(),
                    container_class: "inline-flex max-w-full min-w-0 items-center gap-1".to_string(),
                    button_class: "inline-flex max-w-fit truncate text-left hover:text-emerald-400 transition-colors"
                        .to_string(),
                    separator_class: "text-zinc-500".to_string(),
                }
            }
            // Context menu
            if show_context_menu() {
                div {
                    class: "fixed inset-0 z-[9998]",
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        show_context_menu.set(false);
                    },
                }
                div {
                    class: "fixed z-[9999] w-52 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                    style: anchored_menu_style(menu_x(), menu_y(), 208.0, 360.0),
                    onclick: move |evt: MouseEvent| evt.stop_propagation(),
                    button {
                        class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                        onclick: on_view_album_from_menu,
                        Icon {
                            name: "album".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "View album"
                    }
                    button {
                        class: if shuffle_enabled() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10 hover:bg-emerald-500/20 transition-colors" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                        onclick: on_toggle_shuffle,
                        Icon {
                            name: "shuffle".to_string(),
                            class: if shuffle_enabled() { "w-4 h-4 text-emerald-300".to_string() } else { "w-4 h-4".to_string() },
                        }
                        if shuffle_enabled() {
                            "Shuffle: On"
                        } else {
                            "Shuffle: Off"
                        }
                    }
                    if downloaded() {
                        div { class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10",
                            Icon {
                                name: "check".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                            "Downloaded"
                        }
                    } else {
                        button {
                            class: if download_busy() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-500 cursor-not-allowed" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                            disabled: download_busy(),
                            onclick: make_on_download_album(),
                            Icon {
                                name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                            if download_busy() {
                                "Downloading..."
                            } else {
                                "Download"
                            }
                        }
                    }
                    div { class: "border-t border-zinc-700/60 my-1" }
                    if !album_artist_names.is_empty() {
                        for artist_name in album_artist_names.iter() {
                            button {
                                key: "album-menu-artist-{artist_name}",
                                class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                                onclick: make_on_view_artist_from_menu_named(artist_name.clone()),
                                Icon {
                                    name: "artist".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                if album_artist_names.len() > 1 {
                                    "View {artist_name}"
                                } else {
                                    "View artist"
                                }
                            }
                        }
                    }
                    button {
                        class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                        onclick: on_open_menu_for_context,
                        Icon {
                            name: "plus".to_string(),
                            class: "w-4 h-4".to_string(),
                        }
                        "Add to..."
                    }
                    div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500",
                        "Rating"
                    }
                    div { class: "flex items-center gap-1 px-2 pb-1",
                        for i in 1u32..=5u32 {
                            button {
                                class: "p-1 rounded text-amber-400 hover:text-amber-300 transition-colors",
                                onclick: make_on_set_album_rating(i),
                                Icon {
                                    name: if i <= album_rating() { "star-filled".to_string() } else { "star".to_string() },
                                    class: "w-3.5 h-3.5".to_string(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn SongRow(
    song: Song,
    index: usize,
    onclick: EventHandler<MouseEvent>,
    #[props(default = true)] show_download: bool,
    #[props(default = true)] show_duration: bool,
    #[props(default)] show_favorite_indicator: bool,
    #[props(default)] show_duration_in_menu: bool,
) -> Element {
    let servers = use_context::<Signal<Vec<ServerConfig>>>();
    let navigation = use_context::<Navigation>();
    let queue = use_context::<Signal<Vec<Song>>>();
    let add_menu = use_context::<AddMenuController>();
    let app_settings = use_context::<Signal<AppSettings>>();
    let current_rating = use_signal(|| song.user_rating.unwrap_or(0).min(5));
    let is_favorited = use_signal(|| song.starred.is_some());
    let download_busy = use_signal(|| false);
    let mut show_mobile_actions = use_signal(|| false);
    let initially_downloaded = is_song_downloaded(&song);
    let downloaded = use_signal(move || initially_downloaded);
    let mut menu_x = use_signal(|| 0f64);
    let mut menu_y = use_signal(|| 0f64);

    let cover_url = servers()
        .iter()
        .find(|s| s.id == song.server_id)
        .and_then(|server| {
            let client = NavidromeClient::new(server.clone());
            song.cover_art
                .as_ref()
                .map(|ca| client.get_cover_art_url(ca, 80))
        });

    let album_id = song.album_id.clone();
    let server_id = song.server_id.clone();

    let on_album_click_cover = {
        let album_id = album_id.clone();
        let server_id = server_id.clone();
        let navigation = navigation.clone();
        move |evt: MouseEvent| {
            evt.stop_propagation();
            if let Some(album_id_val) = album_id.clone() {
                navigation.navigate_to(AppView::AlbumDetailView {
                    album_id: album_id_val,
                    server_id: server_id.clone(),
                });
            }
        }
    };

    let make_on_view_album = {
        let navigation = navigation.clone();
        let album_id = song.album_id.clone();
        let server_id = song.server_id.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let navigation = navigation.clone();
            let album_id = album_id.clone();
            let server_id = server_id.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                if let Some(album_id_val) = album_id.clone() {
                    navigation.navigate_to(AppView::AlbumDetailView {
                        album_id: album_id_val,
                        server_id: server_id.clone(),
                    });
                }
            }
        }
    };

    let song_artist_names = parse_artist_names(song.artist.as_deref().unwrap_or_default());
    let direct_song_artist_id = if song_artist_names.len() == 1 {
        song.artist_id.clone()
    } else {
        None
    };
    let make_on_view_artist_named = {
        let servers = servers.clone();
        let navigation = navigation.clone();
        let server_id = song.server_id.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        let direct_song_artist_id = direct_song_artist_id.clone();
        move |artist_name: String| {
            let servers = servers.clone();
            let navigation = navigation.clone();
            let server_id = server_id.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            let direct_song_artist_id = direct_song_artist_id.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                if let Some(artist_id_val) = direct_song_artist_id.clone() {
                    navigation.navigate_to(AppView::ArtistDetailView {
                        artist_id: artist_id_val,
                        server_id: server_id.clone(),
                    });
                    return;
                }
                let server = servers().iter().find(|s| s.id == server_id).cloned();
                let Some(server) = server else {
                    return;
                };
                let navigation = navigation.clone();
                let server_id = server_id.clone();
                let artist_name = artist_name.clone();
                spawn(async move {
                    if let Some(artist_id) = resolve_artist_id_for_name(server, artist_name).await {
                        navigation.navigate_to(AppView::ArtistDetailView {
                            artist_id,
                            server_id,
                        });
                    }
                });
            }
        }
    };

    let make_on_open_menu = {
        let add_menu = add_menu.clone();
        let song = song.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let mut add_menu = add_menu.clone();
            let song = song.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                add_menu.open(AddIntent::from_song(song.clone()));
            }
        }
    };

    let make_on_set_rating = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let current_rating = current_rating.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move |new_rating: u32| {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut current_rating = current_rating.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                let normalized = new_rating.min(5);
                current_rating.set(normalized);
                let servers = servers.clone();
                let song_id = song_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    if let Some(server) = servers().iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let _ = client.set_rating(&song_id, normalized).await;
                    }
                });
            }
        }
    };

    let make_on_toggle_favorite = {
        let servers = servers.clone();
        let song_id = song.id.clone();
        let server_id = song.server_id.clone();
        let queue = queue.clone();
        let is_favorited = is_favorited.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let servers = servers.clone();
            let song_id = song_id.clone();
            let server_id = server_id.clone();
            let mut queue = queue.clone();
            let mut is_favorited = is_favorited.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                let should_star = !is_favorited();
                let servers = servers.clone();
                let song_id = song_id.clone();
                let server_id = server_id.clone();
                spawn(async move {
                    let servers_snapshot = servers();
                    if let Some(server) = servers_snapshot.iter().find(|s| s.id == server_id) {
                        let client = NavidromeClient::new(server.clone());
                        let result = if should_star {
                            client.star(&song_id, "song").await
                        } else {
                            client.unstar(&song_id, "song").await
                        };
                        if result.is_ok() {
                            is_favorited.set(should_star);
                            queue.with_mut(|items| {
                                for s in items.iter_mut() {
                                    if s.id == song_id {
                                        s.starred = if should_star {
                                            Some("local".to_string())
                                        } else {
                                            None
                                        };
                                    }
                                }
                            });
                        }
                    }
                });
            }
        }
    };

    let make_on_download_song = {
        let servers = servers.clone();
        let app_settings = app_settings.clone();
        let song = song.clone();
        let download_busy = download_busy.clone();
        let downloaded = downloaded.clone();
        let show_mobile_actions = show_mobile_actions.clone();
        move || {
            let servers = servers.clone();
            let app_settings = app_settings.clone();
            let song = song.clone();
            let mut download_busy = download_busy.clone();
            let mut downloaded = downloaded.clone();
            let mut show_mobile_actions = show_mobile_actions.clone();
            move |evt: MouseEvent| {
                evt.stop_propagation();
                show_mobile_actions.set(false);
                if download_busy() || downloaded() {
                    return;
                }
                let servers_snapshot = servers();
                if servers_snapshot.is_empty() {
                    return;
                }
                let mut settings_snapshot = app_settings();
                settings_snapshot.downloads_enabled = true;
                download_busy.set(true);
                let song = song.clone();
                spawn(async move {
                    if prefetch_song_audio(&song, &servers_snapshot, &settings_snapshot)
                        .await
                        .is_ok()
                    {
                        downloaded.set(true);
                    }
                    download_busy.set(false);
                });
            }
        }
    };

    rsx! {
        div {
            class: "relative w-full flex items-center gap-4 p-3 rounded-xl hover:bg-zinc-800/50 transition-colors group cursor-pointer",
            onclick: move |e| {
                show_mobile_actions.set(false);
                onclick.call(e);
            },
            // Index
            span { class: "w-6 text-sm text-zinc-500 group-hover:hidden", "{index}" }
            span { class: "w-6 h-6 hidden group-hover:inline-flex items-center justify-center rounded-full bg-emerald-500/95 text-white shadow-lg transition-all group-hover:scale-105 group-hover:-translate-y-0.5",
                Icon {
                    name: "play".to_string(),
                    class: "w-3.5 h-3.5 ml-0.5".to_string(),
                }
            }
            // Cover
            if album_id.is_some() {
                button {
                    class: "w-10 h-10 rounded bg-zinc-800 overflow-hidden flex-shrink-0 pointer-events-none md:pointer-events-auto",
                    aria_label: "Open album",
                    onclick: on_album_click_cover,
                    {
                        match cover_url {
                            Some(url) => rsx! {
                                img { class: "w-full h-full object-cover", src: "{url}" }
                            },
                            None => rsx! {
                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                    Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                                }
                            },
                        }
                    }
                }
            } else {
                div { class: "w-10 h-10 rounded bg-zinc-800 overflow-hidden flex-shrink-0",
                    {
                        match cover_url {
                            Some(url) => rsx! {
                                img { class: "w-full h-full object-cover", src: "{url}" }
                            },
                            None => rsx! {
                                div { class: "w-full h-full flex items-center justify-center bg-gradient-to-br from-zinc-700 to-zinc-800",
                                    Icon { name: "music".to_string(), class: "w-4 h-4 text-zinc-500".to_string() }
                                }
                            },
                        }
                    }
                }
            }
            // Song info
            div { class: "flex-1 min-w-0 text-left",
                div { class: "flex items-center justify-between gap-2 min-w-0",
                    div { class: "flex flex-col min-w-0 flex-1",
                        p { class: "min-w-0 text-sm font-medium text-white truncate group-hover:text-emerald-400 transition-colors",
                            "{song.title}"
                        }
                        div { class: "mt-1 max-w-full inline-flex items-center gap-1 text-xs text-zinc-400",
                            ArtistNameLinks {
                                artist_text: song.artist.clone().unwrap_or_default(),
                                server_id: song.server_id.clone(),
                                fallback_artist_id: song.artist_id.clone(),
                                container_class: "inline-flex max-w-full min-w-0 items-center gap-1".to_string(),
                                button_class: "inline-flex max-w-fit truncate text-left hover:text-emerald-400 transition-colors"
                                    .to_string(),
                                separator_class: "text-zinc-500".to_string(),
                            }
                            if show_download && downloaded() {
                                Icon {
                                    name: "download".to_string(),
                                    class: "w-3 h-3 text-emerald-400 flex-shrink-0".to_string(),
                                }
                            }
                        }
                    }
                    div { class: "flex items-center gap-1 flex-shrink-0 -mr-1",
                        button {
                            class: if is_favorited() { "p-1.5 rounded-lg text-emerald-400 hover:text-emerald-300 hover:bg-emerald-500/10 hover:scale-105 hover:-translate-y-0.5 transition-all" } else { "p-1.5 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 hover:scale-105 hover:-translate-y-0.5 transition-all" },
                            aria_label: if is_favorited() { "Unfavorite" } else { "Favorite" },
                            title: if show_favorite_indicator { if is_favorited() { "Favorited" } else { "Not favorited" } } else if is_favorited() { "Unfavorite" } else { "Favorite" },
                            onclick: make_on_toggle_favorite(),
                            Icon {
                                name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                        }
                        button {
                            class: "p-1.5 rounded-lg text-zinc-500 hover:text-emerald-400 hover:bg-emerald-500/10 hover:scale-105 hover:-translate-y-0.5 transition-all",
                            aria_label: "Song actions",
                            title: "More options",
                            onclick: move |evt: MouseEvent| {
                                evt.stop_propagation();
                                let coords = evt.client_coordinates();
                                menu_x.set(coords.x);
                                menu_y.set(coords.y);
                                show_mobile_actions.set(!show_mobile_actions());
                            },
                            Icon {
                                name: "more-horizontal".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                        }
                    }
                }
                if show_mobile_actions() {
                    div {
                        class: "fixed inset-0 z-[9998]",
                        onclick: move |evt: MouseEvent| {
                            evt.stop_propagation();
                            show_mobile_actions.set(false);
                        },
                    }
                    div {
                        class: "fixed z-[9999] w-44 rounded-xl border border-zinc-700 bg-zinc-900/95 shadow-2xl p-1.5 space-y-1",
                        style: anchored_menu_style(menu_x(), menu_y(), 176.0, 320.0),
                        onclick: move |evt: MouseEvent| evt.stop_propagation(),
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_open_menu(),
                            Icon {
                                name: "plus".to_string(),
                                class: "w-4 h-4".to_string(),
                            }
                            "Add To..."
                        }
                        if song.album_id.is_some() {
                            button {
                                class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                                onclick: make_on_view_album(),
                                Icon {
                                    name: "album".to_string(),
                                    class: "w-4 h-4".to_string(),
                                }
                                "View album"
                            }
                        }
                        if !song_artist_names.is_empty() {
                            for artist_name in song_artist_names.iter() {
                                button {
                                    key: "song-row-menu-artist-{song.id}-{artist_name}",
                                    class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                                    onclick: make_on_view_artist_named(artist_name.clone()),
                                    Icon {
                                        name: "artist".to_string(),
                                        class: "w-4 h-4".to_string(),
                                    }
                                    if song_artist_names.len() > 1 {
                                        "View {artist_name}"
                                    } else {
                                        "View artist"
                                    }
                                }
                            }
                        }
                        if show_download {
                            if downloaded() {
                                div { class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-emerald-300 bg-emerald-500/10",
                                    Icon {
                                        name: "check".to_string(),
                                        class: "w-4 h-4".to_string(),
                                    }
                                    "Downloaded"
                                }
                            } else {
                                button {
                                    class: if download_busy() { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-500 cursor-not-allowed" } else { "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors" },
                                    disabled: download_busy(),
                                    onclick: make_on_download_song(),
                                    Icon {
                                        name: if download_busy() { "loader".to_string() } else { "download".to_string() },
                                        class: "w-4 h-4".to_string(),
                                    }
                                    if download_busy() {
                                        "Downloading..."
                                    } else {
                                        "Download"
                                    }
                                }
                            }
                        }
                        button {
                            class: "w-full flex items-center gap-2 px-2.5 py-2 rounded-lg text-sm text-zinc-200 hover:bg-zinc-800/80 transition-colors",
                            onclick: make_on_toggle_favorite(),
                            Icon {
                                name: if is_favorited() { "heart-filled".to_string() } else { "heart".to_string() },
                                class: "w-4 h-4".to_string(),
                            }
                            if is_favorited() {
                                "Unfavorite"
                            } else {
                                "Favorite"
                            }
                        }
                        div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500",
                            "Rating"
                        }
                        div { class: "flex items-center gap-1 px-2 pb-1",
                            for i in 1..=5 {
                                button {
                                    class: "p-1 rounded text-amber-400 hover:text-amber-300 transition-colors",
                                    onclick: make_on_set_rating(i as u32),
                                    Icon {
                                        name: if i <= current_rating() { "star-filled".to_string() } else { "star".to_string() },
                                        class: "w-3.5 h-3.5".to_string(),
                                    }
                                }
                            }
                        }
                        if show_duration || show_duration_in_menu {
                            div { class: "px-2.5 pt-1 text-[11px] uppercase tracking-wide text-zinc-500",
                                "Length"
                            }
                            p { class: "px-2.5 pb-2 text-xs text-zinc-300",
                                "{format_duration(song.duration)}"
                            }
                        }
                    }
                }
            }
        }
    }
}
