//! Builds without local music (web, Windows, Linux): the same client, always
//! empty. `MusicClient::sources` never includes it there.

use crate::api::{Album, Artist, SearchResult, Song};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LocalLibrary;

const UNSUPPORTED: &str = "Music on this device isn't available here.";

impl LocalLibrary {
    pub async fn get_album_page(&self, _: &str, _: Option<&str>, _: u32, _: u32) -> Result<Vec<Album>, String> {
        Ok(Vec::new())
    }

    pub async fn search_page(&self, _: &str, _: &str, _: u32, _: u32) -> Result<SearchResult, String> {
        Ok(SearchResult::default())
    }

    pub async fn search(&self, _: &str, _: u32, _: u32, _: u32) -> Result<SearchResult, String> {
        Ok(SearchResult::default())
    }

    pub async fn get_native_song_page(&self, _: &str, _: bool, _: usize, _: usize) -> Result<Vec<Song>, String> {
        Ok(Vec::new())
    }

    pub async fn get_artists(&self) -> Result<Vec<Artist>, String> {
        Ok(Vec::new())
    }

    pub async fn get_artist(&self, _: &str) -> Result<(Artist, Vec<Album>), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub async fn get_album(&self, _: &str) -> Result<(Album, Vec<Song>), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub async fn get_top_songs(&self, _: &str, _: u32) -> Result<Vec<Song>, String> {
        Ok(Vec::new())
    }

    pub async fn get_starred(&self) -> Result<(Vec<Artist>, Vec<Album>, Vec<Song>), String> {
        Ok(Default::default())
    }

    pub async fn get_similar_songs(&self, _: &Song, _: u32) -> Result<Vec<Song>, String> {
        Ok(Vec::new())
    }

    pub async fn star(&self, _: &str, _: &str) -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub async fn unstar(&self, _: &str, _: &str) -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub async fn set_rating(&self, _: &str, _: u32) -> Result<(), String> {
        Err(UNSUPPORTED.to_string())
    }

    pub async fn record_play(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}
