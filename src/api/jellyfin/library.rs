// Browsing, search and favourites. Cached like the Subsonic client's calls
// (same lifetimes); one-page list calls for the Library views aren't, as
// the views keep their loaded pages.

use super::dto::{to_album, to_artist, to_playlist, to_song, BaseItemDto, BaseItemDtoQueryResult};
use super::JellyfinClient;
use crate::api::models::{Album, Artist, Playlist, SearchResult, Song};
use crate::cache_service::{get_json as cache_get_json, put_json as cache_put_json, remove_by_prefix as cache_remove_prefix};

/// Fields each list shows (everything else Jellyfin sends by default).
const SONG_FIELDS: &str = "Genres";
const ALBUM_FIELDS: &str = "Genres,DateCreated,ChildCount";
const ARTIST_FIELDS: &str = "ItemCounts";

/// How a Subsonic `getAlbumList2` type sorts on Jellyfin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumListQuery {
    pub sort_by: &'static str,
    pub sort_order: &'static str,
    pub filters: Option<&'static str>,
}

/// Subsonic list types to Jellyfin sorts. "highest" has no user stars to
/// go by, so it's the community rating. Pure; used by tests.
pub fn album_list_query(list_type: &str) -> AlbumListQuery {
    let q = |sort_by, sort_order, filters| AlbumListQuery { sort_by, sort_order, filters };
    match list_type {
        "recent" => q("DatePlayed,SortName", "Descending", Some("IsPlayed")),
        "newest" => q("DateCreated,SortName", "Descending", None),
        "frequent" => q("PlayCount,SortName", "Descending", Some("IsPlayed")),
        "random" => q("Random", "Ascending", None),
        "starred" => q("SortName", "Ascending", Some("IsFavorite")),
        "highest" => q("CommunityRating,SortName", "Descending", None),
        "alphabeticalByArtist" => q("AlbumArtist,SortName", "Ascending", None),
        "byYear" => q("ProductionYear,SortName", "Descending", None),
        _ => q("SortName", "Ascending", None),
    }
}

/// The Songs view's sorts (Navidrome's native field names) on Jellyfin.
/// "rating" is favourites first: Jellyfin has no 1–5 stars.
pub fn song_sort_query(sort: &str) -> &'static str {
    match sort {
        "play_date" => "DatePlayed,SortName",
        "play_count" => "PlayCount,SortName",
        "rating" => "IsFavoriteOrLiked,SortName",
        "duration" => "Runtime,SortName",
        "year" => "ProductionYear,SortName",
        _ => "SortName",
    }
}

/// Query parameters every item list shares.
fn list_params(types: &str, fields: &str) -> Vec<(&'static str, String)> {
    vec![
        ("includeItemTypes", types.to_string()),
        ("recursive", "true".to_string()),
        ("fields", fields.to_string()),
        ("enableUserData", "true".to_string()),
        ("enableImageTypes", "Primary".to_string()),
        ("imageTypeLimit", "1".to_string()),
        ("enableTotalRecordCount", "false".to_string()),
    ]
}

fn page(params: &mut Vec<(&'static str, String)>, size: u32, offset: u32) {
    params.push(("startIndex", offset.to_string()));
    params.push(("limit", size.to_string()));
}

impl JellyfinClient {
    fn cache_key(&self, what: &str, rest: &str) -> String {
        format!("api:jf:{what}:v1:{}:{rest}", self.server.id)
    }

    async fn items(&self, params: &[(&str, String)]) -> Result<Vec<BaseItemDto>, String> {
        Ok(self.get::<BaseItemDtoQueryResult>("/Items", params).await?.into_items())
    }

    async fn item(&self, id: &str, fields: &str) -> Result<BaseItemDto, String> {
        self.get(&format!("/Items/{}", urlencoding::encode(id)), &[("fields", fields.to_string())])
            .await
    }

    fn songs(&self, items: Vec<BaseItemDto>) -> Vec<Song> {
        items.into_iter().map(|i| to_song(i, &self.server.id, &self.server.name)).collect()
    }

    fn albums(&self, items: Vec<BaseItemDto>) -> Vec<Album> {
        items.into_iter().map(|i| to_album(i, &self.server.id)).collect()
    }

    fn artists(&self, items: Vec<BaseItemDto>) -> Vec<Artist> {
        items.into_iter().map(|i| to_artist(i, &self.server.id)).collect()
    }

    /// One page of albums for a Subsonic list type, or a genre.
    pub async fn get_album_page(&self, list_type: &str, genre: Option<&str>, size: u32, offset: u32) -> Result<Vec<Album>, String> {
        let mut params = list_params("MusicAlbum", ALBUM_FIELDS);
        let query = match genre.filter(|g| !g.trim().is_empty()) {
            Some(genre) => {
                params.push(("genres", genre.to_string()));
                album_list_query("alphabeticalByName")
            }
            None => album_list_query(list_type),
        };
        params.push(("sortBy", query.sort_by.to_string()));
        params.push(("sortOrder", query.sort_order.to_string()));
        if let Some(filters) = query.filters {
            params.push(("filters", filters.to_string()));
        }
        page(&mut params, size, offset);
        Ok(self.albums(self.items(&params).await?))
    }

    /// Like [`Self::get_album_page`], cached, for Home and Discover.
    pub async fn get_albums(&self, list_type: &str, size: u32, offset: u32) -> Result<Vec<Album>, String> {
        let key = self.cache_key("albums", &format!("{list_type}:{size}:{offset}"));
        if let Some(cached) = cache_get_json::<Vec<Album>>(&key) {
            return Ok(cached);
        }
        let albums = self.get_album_page(list_type, None, size, offset).await?;
        // "recent" changes with every play (Home refreshes it); "random" each call.
        if !matches!(list_type, "recent" | "random") {
            let _ = cache_put_json(key, &albums, Some(6));
        }
        Ok(albums)
    }

    pub async fn get_album(&self, album_id: &str) -> Result<(Album, Vec<Song>), String> {
        let key = self.cache_key("album", album_id);
        if let Some(cached) = cache_get_json::<(Album, Vec<Song>)>(&key) {
            return Ok(cached);
        }
        let mut params = list_params("Audio", SONG_FIELDS);
        params.push(("parentId", album_id.to_string()));
        params.push(("sortBy", "ParentIndexNumber,IndexNumber,SortName".to_string()));
        let (album, songs) = futures_util::join!(self.item(album_id, ALBUM_FIELDS), self.items(&params));
        let mut album = to_album(album?, &self.server.id);
        let songs = self.songs(songs?);
        if album.song_count == 0 {
            album.song_count = songs.len() as u32;
        }
        let payload = (album, songs);
        let _ = cache_put_json(key, &payload, Some(24));
        Ok(payload)
    }

    pub async fn get_artists(&self) -> Result<Vec<Artist>, String> {
        let key = self.cache_key("artists", "all");
        if let Some(cached) = cache_get_json::<Vec<Artist>>(&key) {
            return Ok(cached);
        }
        // `/Artists/AlbumArtists` is deprecated in 12.4; `/Items` lists them.
        let mut params = list_params("MusicArtist", ARTIST_FIELDS);
        params.push(("sortBy", "SortName".to_string()));
        let artists = self.artists(self.items(&params).await?);
        let _ = cache_put_json(key, &artists, Some(24));
        Ok(artists)
    }

    pub async fn get_artist(&self, artist_id: &str) -> Result<(Artist, Vec<Album>), String> {
        let key = self.cache_key("artist", artist_id);
        if let Some(cached) = cache_get_json::<(Artist, Vec<Album>)>(&key) {
            return Ok(cached);
        }
        let mut params = list_params("MusicAlbum", ALBUM_FIELDS);
        params.push(("albumArtistIds", artist_id.to_string()));
        params.push(("sortBy", "ProductionYear,SortName".to_string()));
        params.push(("sortOrder", "Descending".to_string()));
        let (artist, albums) = futures_util::join!(self.item(artist_id, ARTIST_FIELDS), self.items(&params));
        let mut artist = to_artist(artist?, &self.server.id);
        let albums = self.albums(albums?);
        if artist.album_count == 0 {
            artist.album_count = albums.len() as u32;
        }
        let payload = (artist, albums);
        let _ = cache_put_json(key, &payload, Some(24));
        Ok(payload)
    }

    /// An artist's most played songs here (Jellyfin has no Last.fm top list).
    pub async fn get_top_songs(&self, artist: &str, count: u32) -> Result<Vec<Song>, String> {
        let mut params = list_params("Audio", SONG_FIELDS);
        params.push(("artists", artist.to_string()));
        params.push(("sortBy", "PlayCount,SortName".to_string()));
        params.push(("sortOrder", "Descending".to_string()));
        page(&mut params, count, 0);
        Ok(self.songs(self.items(&params).await?))
    }

    /// One page of every song, sorted by a Songs view sort.
    pub async fn get_native_song_page(&self, sort: &str, ascending: bool, offset: usize, size: usize) -> Result<Vec<Song>, String> {
        let mut params = list_params("Audio", SONG_FIELDS);
        params.push(("sortBy", song_sort_query(sort).to_string()));
        params.push(("sortOrder", if ascending { "Ascending" } else { "Descending" }.to_string()));
        page(&mut params, size as u32, offset as u32);
        Ok(self.songs(self.items(&params).await?))
    }

    pub async fn get_random_songs(&self, size: u32) -> Result<Vec<Song>, String> {
        let mut params = list_params("Audio", SONG_FIELDS);
        params.push(("sortBy", "Random".to_string()));
        page(&mut params, size, 0);
        Ok(self.songs(self.items(&params).await?))
    }

    /// The server's instant mix for a song (Start Radio).
    pub async fn get_similar_songs(&self, id: &str, count: u32) -> Result<Vec<Song>, String> {
        let params = [
            ("limit", count.to_string()),
            ("fields", SONG_FIELDS.to_string()),
            ("enableUserData", "true".to_string()),
            ("enableImageTypes", "Primary".to_string()),
            ("imageTypeLimit", "1".to_string()),
        ];
        let result: BaseItemDtoQueryResult = self
            .get(&format!("/Items/{}/InstantMix", urlencoding::encode(id)), &params)
            .await?;
        Ok(self.songs(result.into_items()))
    }

    /// One page of one kind ("artist", "album" or "song"). An empty query
    /// lists everything A–Z.
    pub async fn search_page(&self, query: &str, kind: &str, size: u32, offset: u32) -> Result<SearchResult, String> {
        let (types, fields) = match kind {
            "artist" => ("MusicArtist", ARTIST_FIELDS),
            "album" => ("MusicAlbum", ALBUM_FIELDS),
            _ => ("Audio", SONG_FIELDS),
        };
        let mut params = list_params(types, fields);
        let query = query.trim();
        if !query.is_empty() {
            params.push(("searchTerm", query.to_string()));
        }
        params.push(("sortBy", "SortName".to_string()));
        page(&mut params, size, offset);
        let items = self.items(&params).await?;
        let mut result = SearchResult::default();
        match kind {
            "artist" => result.artists = self.artists(items),
            "album" => result.albums = self.albums(items),
            _ => result.songs = self.songs(items),
        }
        Ok(result)
    }

    pub async fn search(&self, query: &str, artist_count: u32, album_count: u32, song_count: u32) -> Result<SearchResult, String> {
        let kind_page = |kind: &'static str, count: u32| async move {
            if count == 0 {
                Ok(SearchResult::default())
            } else {
                self.search_page(query, kind, count, 0).await
            }
        };
        let (artists, albums, songs) = futures_util::join!(
            kind_page("artist", artist_count),
            kind_page("album", album_count),
            kind_page("song", song_count),
        );
        Ok(SearchResult {
            artists: artists?.artists,
            albums: albums?.albums,
            songs: songs?.songs,
        })
    }

    pub async fn get_starred(&self) -> Result<(Vec<Artist>, Vec<Album>, Vec<Song>), String> {
        let key = self.cache_key("starred", "all");
        if let Some(cached) = cache_get_json::<(Vec<Artist>, Vec<Album>, Vec<Song>)>(&key) {
            return Ok(cached);
        }
        let favorites = |types: &str, fields: &str| {
            let mut params = list_params(types, fields);
            params.push(("filters", "IsFavorite".to_string()));
            params.push(("sortBy", "SortName".to_string()));
            params
        };
        let (artists, albums, songs) = (
            favorites("MusicArtist", ARTIST_FIELDS),
            favorites("MusicAlbum", ALBUM_FIELDS),
            favorites("Audio", SONG_FIELDS),
        );
        let (artists, albums, songs) = futures_util::join!(self.items(&artists), self.items(&albums), self.items(&songs));
        let payload = (self.artists(artists?), self.albums(albums?), self.songs(songs?));
        let _ = cache_put_json(key, &payload, Some(12));
        Ok(payload)
    }

    /// Favourites any item: a song, album or artist.
    pub async fn star(&self, id: &str, item_type: &str) -> Result<(), String> {
        self.set_favorite(id, item_type, true).await
    }

    pub async fn unstar(&self, id: &str, item_type: &str) -> Result<(), String> {
        self.set_favorite(id, item_type, false).await
    }

    async fn set_favorite(&self, id: &str, item_type: &str, favorite: bool) -> Result<(), String> {
        let method = if favorite { reqwest::Method::POST } else { reqwest::Method::DELETE };
        self.send(method, &format!("/UserFavoriteItems/{}", urlencoding::encode(id)), &[], None)
            .await?;
        self.invalidate_after_star(id, item_type);
        Ok(())
    }

    /// Drops cached responses that carry the old favourite mark (a song's
    /// shows inside its cached album too).
    fn invalidate_after_star(&self, id: &str, item_type: &str) {
        let _ = cache_remove_prefix(&self.cache_key("starred", ""));
        let _ = cache_remove_prefix("view:favorites:v1:");
        let _ = match item_type {
            "album" => cache_remove_prefix(&self.cache_key("album", id)),
            "artist" => cache_remove_prefix(&self.cache_key("artist", id)),
            _ => cache_remove_prefix(&self.cache_key("album", "")),
        };
    }

    pub async fn get_playlists(&self) -> Result<Vec<Playlist>, String> {
        let key = self.cache_key("playlists", "all");
        if let Some(cached) = cache_get_json::<Vec<Playlist>>(&key) {
            return Ok(cached);
        }
        let mut params = list_params("Playlist", "ChildCount,DateCreated,Overview");
        params.push(("mediaTypes", "Audio".to_string()));
        params.push(("sortBy", "SortName".to_string()));
        let playlists: Vec<Playlist> = self
            .items(&params)
            .await?
            .into_iter()
            .map(|i| to_playlist(i, &self.server.id))
            .collect();
        let _ = cache_put_json(key, &playlists, Some(6));
        Ok(playlists)
    }

    pub async fn get_playlist(&self, playlist_id: &str) -> Result<(Playlist, Vec<Song>), String> {
        let key = self.cache_key("playlist", playlist_id);
        if let Some(cached) = cache_get_json::<(Playlist, Vec<Song>)>(&key) {
            return Ok(cached);
        }
        let params = [
            ("fields", SONG_FIELDS.to_string()),
            ("enableUserData", "true".to_string()),
            ("enableImageTypes", "Primary".to_string()),
            ("imageTypeLimit", "1".to_string()),
        ];
        let path = format!("/Playlists/{}/Items", urlencoding::encode(playlist_id));
        let (playlist, entries) = futures_util::join!(
            self.item(playlist_id, "ChildCount,DateCreated,Overview"),
            self.get::<BaseItemDtoQueryResult>(&path, &params)
        );
        let mut playlist = to_playlist(playlist?, &self.server.id);
        let songs = self.songs(entries?.into_items());
        playlist.song_count = songs.len() as u32;
        playlist.duration = songs.iter().map(|s| s.duration).sum();
        let payload = (playlist, songs);
        let _ = cache_put_json(key, &payload, Some(6));
        Ok(payload)
    }

    pub fn refresh_playlist_cache(&self) {
        let _ = cache_remove_prefix(&self.cache_key("playlists", ""));
        let _ = cache_remove_prefix(&self.cache_key("playlist", ""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsonic_list_types_map_to_jellyfin_sorts() {
        let recent = album_list_query("recent");
        assert_eq!((recent.sort_by, recent.sort_order, recent.filters), ("DatePlayed,SortName", "Descending", Some("IsPlayed")));
        assert_eq!(album_list_query("newest").sort_by, "DateCreated,SortName");
        assert_eq!(album_list_query("random").sort_by, "Random");
        assert_eq!(album_list_query("starred").filters, Some("IsFavorite"));
        assert_eq!(album_list_query("whatever").sort_by, "SortName");
        assert_eq!(song_sort_query("play_count"), "PlayCount,SortName");
        assert_eq!(song_sort_query("title"), "SortName");
    }
}
