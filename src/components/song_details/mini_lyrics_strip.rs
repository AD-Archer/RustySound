// Compact, always-visible lyrics preview strip.

#[derive(Clone, PartialEq)]
struct MiniLyricsPreviewData {
    previous: Option<String>,
    current: String,
    next: Option<String>,
}

#[derive(Props, Clone, PartialEq)]
struct MiniLyricsStripProps {
    preview: Option<MiniLyricsPreviewData>,
    is_live_stream: bool,
}

#[component]
fn MiniLyricsStrip(props: MiniLyricsStripProps) -> Element {
    let controller = use_context::<SongDetailsController>();
    let on_open_lyrics = {
        let mut controller = controller.clone();
        move |_| controller.set_tab(SongDetailsTab::Lyrics)
    };

    let (previous, current, next) = if props.is_live_stream {
        (
            Some("Live stream".to_string()),
            "Synced lyric preview is disabled".to_string(),
            Some("Tap to open full lyrics tools".to_string()),
        )
    } else if let Some(preview) = props.preview {
        let previous = preview
            .previous
            .filter(|line| !line.trim().is_empty())
            .unwrap_or_else(|| " ".to_string());
        let current = if preview.current.trim().is_empty() {
            "Lyrics unavailable".to_string()
        } else {
            preview.current
        };
        let next = preview
            .next
            .filter(|line| !line.trim().is_empty())
            .unwrap_or_else(|| " ".to_string());
        (Some(previous), current, Some(next))
    } else {
        (
            Some("No lyrics loaded".to_string()),
            "Tap to open lyrics".to_string(),
            Some("Search and pick a better match".to_string()),
        )
    };

    rsx! {
        button {
            class: "w-full rounded-xl border border-zinc-800/80 bg-zinc-900/95 text-left px-3 py-2 space-y-1 overflow-hidden",
            onclick: on_open_lyrics,
            p { class: "text-[11px] uppercase tracking-[0.18em] text-zinc-500", "Quick Lyrics" }
            if let Some(previous) = previous {
                p { class: "text-xs text-zinc-500 truncate leading-snug", "{previous}" }
            }
            p { class: "text-sm text-emerald-300 font-medium truncate leading-snug", "{current}" }
            if let Some(next) = next {
                p { class: "text-xs text-zinc-400 truncate leading-snug", "{next}" }
            }
        }
    }
}

