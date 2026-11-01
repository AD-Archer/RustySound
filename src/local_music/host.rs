//! The local library's worker, alive for the whole app.
//!
//! One queue of commands (add, remove, rescan, "a folder changed"), handled
//! one at a time. Scans run on the blocking pool; while one reads files the
//! worker publishes `(read, to read)` to [`STATUS`] every few hundred ms,
//! which Settings shows as quiet progress. The index generation is bumped
//! when a scan changed something, and once early in a first scan so the
//! library isn't empty while it runs.
//!
//! * Mac: folders come from Settings; FSEvents (`notify`) report changes,
//!   debounced, and the changed folder is rescanned (unchanged files cost a
//!   `stat`).
//! * iPhone: the app's Documents folder (shown in the Files app) is the one
//!   folder; it's rescanned on launch and whenever the app comes back to the
//!   foreground.

use super::index::{self, Folder};
use super::library::{self, forget_paths, run};
use super::scan::{scan_folder, ScanReport};
use super::{FolderInfo, SONG_COUNT, STATUS};
use dioxus::prelude::*;
use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(target_os = "ios", allow(dead_code))]
pub enum Command {
    ScanAll,
    Add(String),
    Remove(String),
    /// A watched folder reported changes (Mac).
    Changed(String),
}

/// Watch events arrive in bursts (a copy is many writes): wait this long
/// after the first one before rescanning.
const SETTLE_MS: u64 = 1_500;
const PROGRESS_MS: u64 = 300;

static SENDER: Lazy<Mutex<Option<UnboundedSender<Command>>>> = Lazy::new(|| Mutex::new(None));
static FRESH: Lazy<Mutex<HashSet<String>>> = Lazy::new(|| Mutex::new(HashSet::new()));

pub fn send(command: Command) {
    if let Some(tx) = SENDER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.send(command);
    }
}

pub fn take_fresh(id: &str) -> bool {
    let mut fresh = FRESH.lock().unwrap_or_else(|e| e.into_inner());
    !fresh.is_empty() && fresh.remove(id)
}

pub fn use_host() {
    use_hook(|| {
        let (tx, rx) = unbounded_channel();
        *SENDER.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
        spawn(worker(rx));
        send(Command::ScanAll);
    });
    #[cfg(all(feature = "mobile", target_os = "ios"))]
    use_future(|| async {
        let mut lifecycle = document::eval(
            r#"document.addEventListener("visibilitychange", () => { if (!document.hidden) dioxus.send(true); });
await new Promise(() => {});"#,
        );
        while lifecycle.recv::<bool>().await.is_ok() {
            send(Command::ScanAll);
        }
    });
    #[cfg(all(feature = "desktop", target_os = "macos"))]
    use_future(|| async {
        // Ogg Vorbis and Opus play only on newer WebKit; ask the webview.
        let probe = document::eval(
            r#"const a = document.createElement("audio");
return a.canPlayType('audio/ogg; codecs="vorbis"') !== "";"#,
        );
        if let Ok(yes) = probe.await {
            if let Ok(yes) = serde_json::from_value::<bool>(yes) {
                super::model::set_webkit_plays_ogg(yes);
            }
        }
    });
}

/// The folders, as Settings lists them.
fn folder_infos(folders: &[Folder]) -> Vec<FolderInfo> {
    folders
        .iter()
        .map(|f| FolderInfo {
            id: f.id.clone(),
            path: f.path.clone(),
            removable: cfg!(not(target_os = "ios")),
        })
        .collect()
}

/// Re-reads folders and the song count into [`STATUS`]; `bump` also moves
/// the generation so library views refetch.
async fn publish(bump: bool) {
    let (folders, songs) = tokio::task::spawn_blocking(|| (library::folders(), library::track_count()))
        .await
        .unwrap_or_default();
    SONG_COUNT.store(songs, Ordering::Relaxed);
    let infos = folder_infos(&folders);
    let mut status = STATUS.write();
    status.folders = infos;
    status.songs = songs;
    if bump {
        status.generation += 1;
    }
}

/// The iPhone's one folder: Documents, under an id that survives the
/// container path changing on reinstall.
#[cfg(all(feature = "mobile", target_os = "ios"))]
async fn register_documents() {
    let Some(home) = dirs::home_dir() else { return };
    let path = home.join("Documents").to_string_lossy().into_owned();
    let _ = run(move |c| index::set_folder(c, &super::model::folder_id("ios-documents"), &path)).await;
}

async fn worker(mut rx: UnboundedReceiver<Command>) {
    #[cfg(all(feature = "mobile", target_os = "ios"))]
    register_documents().await;
    // Views that loaded before the count was known refetch once, and only
    // when there is local music to add.
    publish(false).await;
    if SONG_COUNT.load(Ordering::Relaxed) > 0 {
        STATUS.write().generation += 1;
    }
    #[cfg(all(feature = "desktop", target_os = "macos"))]
    let mut watcher = watch::Watcher::default();
    #[cfg(all(feature = "desktop", target_os = "macos"))]
    watcher.watch(&library_folders().await);

    while let Some(first) = rx.recv().await {
        if matches!(first, Command::Changed(_)) {
            crate::time::sleep_ms(SETTLE_MS).await;
        }
        let mut commands = vec![first];
        while let Ok(next) = rx.try_recv() {
            commands.push(next);
        }

        let mut scan_all = false;
        let mut targets: Vec<String> = Vec::new();
        let mut folders_changed = false;
        for command in commands {
            match command {
                Command::ScanAll => scan_all = true,
                Command::Changed(id) => targets.push(id),
                Command::Add(path) => {
                    if let Ok(folder) = run(move |c| index::add_folder(c, &path)).await {
                        targets.push(folder.id);
                        folders_changed = true;
                    }
                }
                Command::Remove(id) => {
                    let _ = run(move |c| index::remove_folder(c, &id)).await;
                    forget_paths();
                    folders_changed = true;
                }
            }
        }

        let folders = library_folders().await;
        if folders_changed {
            publish(true).await;
            #[cfg(all(feature = "desktop", target_os = "macos"))]
            watcher.watch(&folders);
        }
        let mut changed = false;
        let mut fresh = Vec::new();
        for folder in folders.into_iter().filter(|f| scan_all || targets.contains(&f.id)) {
            let report = scan_with_progress(folder).await;
            changed |= report.read > 0 || report.removed > 0;
            fresh.extend(report.fresh);
        }
        if STATUS.peek().scanning.is_some() {
            STATUS.write().scanning = None;
        }
        if changed {
            forget_paths();
            *FRESH.lock().unwrap_or_else(|e| e.into_inner()) = fresh.into_iter().collect();
            publish(true).await;
        }
    }
}

async fn library_folders() -> Vec<Folder> {
    tokio::task::spawn_blocking(library::folders).await.unwrap_or_default()
}

/// Scans one folder on the blocking pool, publishing progress meanwhile.
async fn scan_with_progress(folder: Folder) -> ScanReport {
    let progress = Arc::new((AtomicUsize::new(0), AtomicUsize::new(0)));
    let art_dir = crate::offline_art::cover_art_dir();
    let job = {
        let progress = progress.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = super::library::open()?;
            scan_folder(&mut conn, &folder, art_dir.as_deref(), &|done, total| {
                progress.0.store(done, Ordering::Relaxed);
                progress.1.store(total, Ordering::Relaxed);
            })
            .map_err(|e| e.to_string())
        })
    };
    tokio::pin!(job);
    let empty_before = SONG_COUNT.load(Ordering::Relaxed) == 0;
    let mut shown_early = false;
    loop {
        tokio::select! {
            result = &mut job => {
                return result.ok().and_then(Result::ok).unwrap_or_default();
            }
            _ = crate::time::sleep_ms(PROGRESS_MS) => {
                let (done, total) = (progress.0.load(Ordering::Relaxed), progress.1.load(Ordering::Relaxed));
                let next = (total > 0).then_some((done, total));
                if STATUS.peek().scanning != next {
                    STATUS.write().scanning = next;
                }
                // A first scan: show what's there after the first batches
                // instead of an empty library until the end.
                if empty_before && !shown_early && done >= 200 {
                    shown_early = true;
                    publish(true).await;
                }
            }
        }
    }
}

#[cfg(all(feature = "desktop", target_os = "macos"))]
mod watch {
    //! FSEvents through `notify`. Each event names a path; the folder that
    //! contains it is queued for a rescan (the worker debounces).

    use super::{send, Command, Folder};
    use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

    #[derive(Default)]
    pub struct Watcher {
        inner: Option<RecommendedWatcher>,
    }

    impl Watcher {
        /// Replaces the watched set with `folders`.
        pub fn watch(&mut self, folders: &[Folder]) {
            self.inner = None;
            if folders.is_empty() {
                return;
            }
            let roots: Vec<(String, std::path::PathBuf)> = folders
                .iter()
                .map(|f| (f.id.clone(), std::path::PathBuf::from(&f.path)))
                .collect();
            let lookup = roots.clone();
            let handler = move |result: notify::Result<Event>| {
                let Ok(event) = result else { return };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                for path in &event.paths {
                    if let Some((id, _)) = lookup.iter().find(|(_, root)| path.starts_with(root)) {
                        send(Command::Changed(id.clone()));
                    }
                }
            };
            let Ok(mut watcher) = notify::recommended_watcher(handler) else {
                return;
            };
            for (_, root) in &roots {
                let _ = watcher.watch(root, RecursiveMode::Recursive);
            }
            self.inner = Some(watcher);
        }
    }
}
