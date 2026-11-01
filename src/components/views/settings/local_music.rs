//! Settings → Servers → On This Device: the folders the local library reads
//! (Mac: add with the folder picker, remove; iPhone: the Documents folder the
//! Files app shows), and a quiet line about the library or a running scan.
//! Only in builds with local music; the web build never mounts it.

use super::shared::{ActionRow, Footnote, GroupTitle};
use crate::components::Icon;
use crate::local_music::{self, FolderInfo, STATUS};
use dioxus::prelude::*;

/// "1,240".
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn songs(n: usize) -> String {
    if n == 1 {
        "1 song".to_string()
    } else {
        format!("{} songs", grouped(n))
    }
}

/// The line under the folders: progress while a scan reads files, else
/// what's there and how to add more.
fn status_line(songs_now: usize, scanning: Option<(usize, usize)>, has_folders: bool) -> String {
    let device = if cfg!(target_os = "ios") { "iPhone" } else { "Mac" };
    if let Some((done, _)) = scanning {
        return format!("Scanning… {}", songs(done));
    }
    match (songs_now, has_folders) {
        (0, _) if cfg!(target_os = "ios") => {
            "Add music in the Files app, under On My iPhone → RustySound. It shows up here when you come back.".to_string()
        }
        (0, false) => "Add a folder of music files to play them beside your server's songs.".to_string(),
        (0, true) => "No music files found yet.".to_string(),
        (n, _) if cfg!(target_os = "ios") => {
            format!("{} on this {device}. Add more in the Files app, under On My iPhone → RustySound.", songs(n))
        }
        (n, _) => format!(
            "{} on this {device}. New and changed files are picked up automatically; removing a folder never deletes files.",
            songs(n)
        ),
    }
}

/// The last part of a path, for a folder row's title.
fn folder_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
}

#[component]
pub(super) fn LocalMusicGroup() -> Element {
    let status = STATUS.read().clone();
    let has_folders = !status.folders.is_empty();
    let line = status_line(status.songs, status.scanning, has_folders);

    rsx! {
        GroupTitle { title: local_music::LOCAL_SOURCE_NAME.to_string() }
        if has_folders || cfg!(not(target_os = "ios")) {
            ul { class: "rs-list rs-settings-folders", aria_label: "Music folders", "data-testid": "local-folders",
                for folder in status.folders.iter().cloned() {
                    FolderRow { key: "{folder.id}", folder }
                }
                if cfg!(not(target_os = "ios")) {
                    ActionRow {
                        label: "Add Music Folder…".to_string(),
                        icon: "plus".to_string(),
                        testid: "local-folder-add".to_string(),
                        onclick: move |_| local_music::pick_and_add_folder(),
                    }
                }
                if has_folders {
                    ActionRow {
                        label: "Look for New Music".to_string(),
                        icon: "refresh-cw".to_string(),
                        busy: status.scanning.is_some(),
                        testid: "local-rescan".to_string(),
                        onclick: move |_| local_music::rescan(),
                    }
                }
            }
        }
        Footnote {
            span { class: "rs-settings-scan", role: "status", aria_live: "polite", "{line}" }
        }
    }
}

#[component]
fn FolderRow(folder: FolderInfo) -> Element {
    let name = if cfg!(target_os = "ios") {
        "Documents (Files app)".to_string()
    } else {
        folder_name(&folder.path)
    };
    let title_id = format!("folder-{}-name", folder.id);
    rsx! {
        li { "data-testid": "local-folder",
            div { class: "rs-row rs-row--compact rs-settings-folder",
                span { class: "rs-row__lead", aria_hidden: "true",
                    Icon { name: "folder".to_string(), class: "rs-ic rs-ic--md".to_string() }
                }
                span { class: "rs-row__meta",
                    span { class: "rs-row__title", id: "{title_id}", "{name}" }
                    span { class: "rs-row__sub", style: "display:block", "{folder.path}" }
                }
                if folder.removable {
                    button {
                        r#type: "button",
                        class: "rs-btn rs-btn--plain rs-settings-folder__remove",
                        "data-testid": "local-folder-remove",
                        aria_describedby: "{title_id}",
                        onclick: {
                            let id = folder.id.clone();
                            move |_| local_music::remove_folder(&id)
                        },
                        "Remove"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_read_naturally() {
        assert_eq!(grouped(7), "7");
        assert_eq!(grouped(1240), "1,240");
        assert_eq!(grouped(22_015), "22,015");
        assert_eq!(grouped(1_000_000), "1,000,000");
        assert_eq!(songs(1), "1 song");
        assert_eq!(status_line(0, Some((1240, 5000)), true), "Scanning… 1,240 songs");
        assert_eq!(folder_name("/Users/me/Music/Lossless"), "Lossless");
    }
}
