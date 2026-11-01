//! Where the playing song came from, for the cover and title taps in Now
//! Playing: the playlist or RustySound mix it's playing from, otherwise its
//! album.

use crate::api::discovery::mix_slug;
use crate::api::{QueueSourceKind, Song};
use crate::components::AppView;

/// The page a tap on the cover or title opens, and how to describe it.
#[derive(Clone, PartialEq)]
pub(super) struct SourcePage {
    pub view: AppView,
    /// "Open playlist", "Open mix" or "Open album", for the accessible name.
    pub verb: &'static str,
}

/// Playing from a playlist or a Home mix opens that; anything else opens the
/// song's album. None for radio or a song without an album.
pub(super) fn source_page(song: &Song) -> Option<SourcePage> {
    if let Some(meta) = song.queue_meta.as_ref() {
        match meta.source_kind {
            // Playlist sources are "{server_id}::{playlist_id}".
            QueueSourceKind::Playlist => {
                if let Some((server_id, playlist_id)) = meta.source_id.split_once("::") {
                    if !server_id.is_empty() && !playlist_id.is_empty() {
                        return Some(SourcePage {
                            view: AppView::PlaylistDetailView {
                                playlist_id: playlist_id.to_string(),
                                server_id: server_id.to_string(),
                            },
                            verb: "Open playlist",
                        });
                    }
                }
            }
            // Home mixes are "discover:{mix id}"; other random mixes have no page.
            QueueSourceKind::RandomMix => {
                if let Some(mix_id) = meta.source_id.strip_prefix("discover:") {
                    if !mix_id.is_empty() {
                        return Some(SourcePage {
                            view: AppView::MixDetailView { mix_id: mix_slug(mix_id) },
                            verb: "Open mix",
                        });
                    }
                }
            }
            _ => {}
        }
    }
    let album_id = song.album_id.as_deref().filter(|id| !id.is_empty())?;
    if song.server_id.is_empty() {
        return None;
    }
    Some(SourcePage {
        view: AppView::AlbumDetailView {
            album_id: album_id.to_string(),
            server_id: song.server_id.clone(),
        },
        verb: "Open album",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::QueueSongMeta;

    fn song(album: Option<&str>, meta: Option<(QueueSourceKind, &str)>) -> Song {
        Song {
            id: "s1".into(),
            server_id: "srv".into(),
            album_id: album.map(String::from),
            queue_meta: meta.map(|(source_kind, source_id)| QueueSongMeta {
                group_id: "g".into(),
                source_kind,
                source_id: source_id.into(),
                source_position: 0,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn playlist_source_opens_the_playlist() {
        let page = source_page(&song(Some("al1"), Some((QueueSourceKind::Playlist, "srv::pl9")))).unwrap();
        assert!(page.view == AppView::PlaylistDetailView { playlist_id: "pl9".into(), server_id: "srv".into() });
        assert_eq!(page.verb, "Open playlist");
    }

    #[test]
    fn home_mix_opens_the_mix_page() {
        let page = source_page(&song(Some("al1"), Some((QueueSourceKind::RandomMix, "discover:discovery")))).unwrap();
        assert!(page.view == AppView::MixDetailView { mix_id: mix_slug("discovery") });
    }

    #[test]
    fn album_artist_and_other_sources_open_the_album() {
        for kind in [QueueSourceKind::Album, QueueSourceKind::Artist, QueueSourceKind::Favorites] {
            let page = source_page(&song(Some("al1"), Some((kind, "whatever")))).unwrap();
            assert!(page.view == AppView::AlbumDetailView { album_id: "al1".into(), server_id: "srv".into() });
        }
        // A random mix that isn't a Home mix has no page of its own.
        let page = source_page(&song(Some("al1"), Some((QueueSourceKind::RandomMix, "random:1")))).unwrap();
        assert_eq!(page.verb, "Open album");
        assert!(source_page(&song(None, None)).is_none());
    }
}
