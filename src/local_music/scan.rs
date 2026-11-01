//! Walks a music folder and reads tags with lofty, for the Mac and iPhone alike.
//!
//! Incremental: a file is read only when it's new or its mtime or size
//! changed ([`needs_rescan`]), and files that disappeared leave the index, so
//! a rescan of an unchanged library costs one `stat` per file. Tags are read
//! outside any transaction and written in batches: the database is locked for
//! milliseconds at a time and the app's own writes never wait long.

use super::index::{self, Folder};
use super::model::{self, needs_rescan, TrackRecord};
use lofty::config::ParseOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::PictureType;
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Files written per transaction.
const BATCH: usize = 100;
/// Deeper than any real library; stops a pathological tree.
const MAX_DEPTH: usize = 32;
/// Embedded lyrics longer than this are someone's liner notes, not lyrics.
const MAX_LYRICS_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanReport {
    /// Audio files found in the folder.
    pub found: usize,
    /// New or changed files whose tags were read.
    pub read: usize,
    pub removed: usize,
    /// Files lofty couldn't parse (left out of the index).
    pub failed: usize,
    /// Song and album ids of files seen for the first time (capped), so
    /// their rows can fade in.
    pub fresh: Vec<String>,
}

/// New ids kept per scan; a first scan of a big library fades only the start.
const MAX_FRESH: usize = 2_000;

/// One audio file on disk.
struct FoundFile {
    rel_path: String,
    path: PathBuf,
    mtime: i64,
    size: u64,
}

/// Brings the index of `folder` up to date with the disk.
/// * `art_dir`: where embedded pictures are written (the cover cache);
///   `None` skips pictures.
/// * `progress(done, total)`: called after each batch of files read.
pub fn scan_folder(
    conn: &mut Connection,
    folder: &Folder,
    art_dir: Option<&Path>,
    progress: &dyn Fn(usize, usize),
) -> rusqlite::Result<ScanReport> {
    let root = PathBuf::from(&folder.path);
    let mut found = Vec::new();
    walk(&root, &root, 0, &mut found);

    let known = index::known_files(conn, &folder.id)?;
    let on_disk: HashSet<&str> = found.iter().map(|f| f.rel_path.as_str()).collect();
    let gone: Vec<String> = known
        .keys()
        .filter(|rel| !on_disk.contains(rel.as_str()))
        .cloned()
        .collect();
    index::delete_files(conn, &folder.id, &gone)?;

    let changed: Vec<&FoundFile> = found
        .iter()
        .filter(|f| needs_rescan(known.get(&f.rel_path).copied(), f.mtime, f.size))
        .collect();
    let mut report = ScanReport {
        found: found.len(),
        removed: gone.len(),
        ..Default::default()
    };
    progress(0, changed.len());
    for (batch_index, batch) in changed.chunks(BATCH).enumerate() {
        let records: Vec<TrackRecord> = batch
            .iter()
            .filter_map(|file| {
                let record = read_track(file, &folder.id, art_dir);
                if record.is_none() {
                    report.failed += 1;
                }
                record
            })
            .collect();
        report.read += records.len();
        for record in records.iter().filter(|r| !known.contains_key(&r.rel_path)) {
            if report.fresh.len() < MAX_FRESH {
                report.fresh.push(record.id.clone());
                report.fresh.push(record.album_id());
            }
        }
        index::upsert_tracks(conn, &records)?;
        progress((batch_index * BATCH + batch.len()).min(changed.len()), changed.len());
    }
    Ok(report)
}

/// Collects audio files under `dir`. Hidden entries and symlinks are skipped
/// (no loops, no surprises from aliases into other volumes).
fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<FoundFile>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            walk(root, &path, depth + 1, out);
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let is_audio = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(model::is_indexed_extension);
        if !is_audio {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let Ok(rel) = path.strip_prefix(root) else { continue };
        let rel_path = rel
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect::<Vec<_>>()
            .join("/");
        out.push(FoundFile {
            rel_path,
            path,
            mtime,
            size: meta.len(),
        });
    }
}

fn clean(value: Option<impl AsRef<str>>) -> Option<String> {
    value
        .map(|v| v.as_ref().trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Reads one file's tags and properties. `None` when lofty can't parse it.
fn read_track(file: &FoundFile, folder_id: &str, art_dir: Option<&Path>) -> Option<TrackRecord> {
    let options = ParseOptions::new().read_cover_art(art_dir.is_some());
    let tagged = Probe::open(&file.path).ok()?.options(options).read().ok()?;
    let properties = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let suffix = file
        .path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let stem = file
        .path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();
    // Untagged files in "Artist/Album/01 Song.mp3" still land in an album.
    let parent_album = file
        .rel_path
        .rsplit_once('/')
        .and_then(|(dir, _)| dir.rsplit('/').next())
        .map(str::to_string);

    let mut record = TrackRecord {
        id: model::song_id(folder_id, &file.rel_path),
        folder_id: folder_id.to_string(),
        rel_path: file.rel_path.clone(),
        mtime: file.mtime,
        size: file.size,
        title: stem,
        duration: properties.duration().as_secs_f64().round() as u32,
        bitrate: properties.audio_bitrate().or(properties.overall_bitrate()).filter(|b| *b > 0),
        content_type: model::content_type_for(&suffix).to_string(),
        suffix,
        album: parent_album,
        ..Default::default()
    };

    if let Some(tag) = tag {
        if let Some(title) = clean(tag.title()) {
            record.title = title;
        }
        record.artist = clean(tag.artist());
        record.album_artist = clean(tag.get_string(ItemKey::AlbumArtist));
        if let Some(album) = clean(tag.album()) {
            record.album = Some(album);
        }
        record.track = tag.track().filter(|n| *n > 0);
        record.disc = tag.disk().filter(|n| *n > 0);
        record.year = tag.date().map(|d| d.year as u32).filter(|y| *y > 0);
        record.genre = clean(tag.genre());
        record.mbid = clean(tag.get_string(ItemKey::MusicBrainzRecordingId));
        record.replay_gain = model::replay_gain_from_tags(
            tag.get_string(ItemKey::ReplayGainTrackGain),
            tag.get_string(ItemKey::ReplayGainAlbumGain),
            tag.get_string(ItemKey::ReplayGainTrackPeak),
            tag.get_string(ItemKey::ReplayGainAlbumPeak),
        );
        record.lyrics = clean(
            tag.get_string(ItemKey::Lyrics)
                .or_else(|| tag.get_string(ItemKey::UnsyncLyrics)),
        )
        .filter(|text| text.len() <= MAX_LYRICS_BYTES);
        if let Some(dir) = art_dir {
            let picture = tag
                .pictures()
                .iter()
                .find(|p| p.pic_type() == PictureType::CoverFront)
                .or_else(|| tag.pictures().first());
            if let Some(picture) = picture.filter(|p| !p.data().is_empty()) {
                let id = model::art_id(picture.data());
                let path = dir.join(model::art_file_name(&id));
                // Written once per distinct picture; an album's tracks share it.
                if path.exists() || std::fs::write(&path, picture.data()).is_ok() {
                    record.art_id = Some(id);
                }
            }
        }
    }
    Some(record)
}

/// The embedded picture of `path`, to rebuild a cover after the cache was
/// cleared. Same choice as the scanner: the front cover, else the first.
pub fn read_picture(path: &Path) -> Option<Vec<u8>> {
    let tagged = Probe::open(path).ok()?.read().ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let picture = tag
        .pictures()
        .iter()
        .find(|p| p.pic_type() == PictureType::CoverFront)
        .or_else(|| tag.pictures().first())?;
    Some(picture.data().to_vec())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::local_music::index::tests::memory;
    use lofty::config::WriteOptions;
    use lofty::picture::{MimeType, Picture};
    use lofty::tag::{Tag, TagExt, TagType};
    use std::cell::RefCell;

    /// A scratch directory under the system temp dir, removed on drop.
    pub(crate) struct TempDir(pub PathBuf);

    impl TempDir {
        pub(crate) fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "rustysound-local-{name}-{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A one-second silent 8 kHz mono WAV: 16 KB, decodable everywhere.
    pub(crate) fn wav_bytes() -> Vec<u8> {
        let rate: u32 = 8_000;
        let data_len: u32 = rate * 2;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        out.resize(44 + data_len as usize, 0);
        out
    }

    /// Writes a tagged WAV (ID3v2 chunk) at `dir/rel`.
    pub(crate) fn write_song(dir: &Path, rel: &str, title: &str, artist: &str, album: &str, track: u32, art: Option<&[u8]>) -> PathBuf {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, wav_bytes()).unwrap();
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title(title.to_string());
        tag.set_artist(artist.to_string());
        tag.set_album(album.to_string());
        tag.set_track(track);
        tag.insert_text(ItemKey::RecordingDate, "1999".to_string());
        tag.set_genre("Test".to_string());
        tag.insert_text(ItemKey::UnsyncLyrics, "la la la".to_string());
        tag.insert_text(ItemKey::ReplayGainTrackGain, "-4.25 dB".to_string());
        if let Some(bytes) = art {
            tag.push_picture(
                Picture::unchecked(bytes.to_vec())
                    .pic_type(PictureType::CoverFront)
                    .mime_type(MimeType::Png)
                    .build(),
            );
        }
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        path
    }

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];

    #[test]
    fn scans_tags_art_and_lyrics() {
        let music = TempDir::new("scan");
        let art = TempDir::new("scan-art");
        write_song(&music.0, "Band/Bee/01.wav", "First", "Band", "Bee", 1, Some(PNG));
        write_song(&music.0, "Band/Bee/02.wav", "Second", "Band", "Bee", 2, Some(PNG));
        std::fs::write(music.0.join("notes.txt"), "not audio").unwrap();
        std::fs::write(music.0.join(".hidden.wav"), wav_bytes()).unwrap();
        std::fs::write(music.0.join("broken.flac"), b"not a flac").unwrap();

        let mut conn = memory();
        let folder = index::add_folder(&conn, music.0.to_str().unwrap()).unwrap();
        let calls = RefCell::new(Vec::new());
        let report = scan_folder(&mut conn, &folder, Some(&art.0), &|done, total| {
            calls.borrow_mut().push((done, total))
        })
        .unwrap();
        assert_eq!(report.found, 3);
        assert_eq!(report.read, 2);
        assert_eq!(report.failed, 1);
        assert_eq!(calls.borrow().last(), Some(&(3, 3)));

        let songs = index::songs_page(&conn, "title", true, 0, 10).unwrap();
        assert_eq!(songs.len(), 2);
        let first = &songs[0];
        assert_eq!(first.title, "First");
        assert_eq!(first.artist.as_deref(), Some("Band"));
        assert_eq!(first.album.as_deref(), Some("Bee"));
        assert_eq!(first.track, Some(1));
        assert_eq!(first.year, Some(1999));
        assert_eq!(first.genre.as_deref(), Some("Test"));
        assert_eq!(first.duration, 1);
        assert_eq!(first.suffix.as_deref(), Some("wav"));
        assert_eq!(first.content_type.as_deref(), Some("audio/wav"));
        assert_eq!(first.id, model::song_id(&folder.id, "Band/Bee/01.wav"));
        // One picture file for both tracks.
        let art_id = first.cover_art.clone().unwrap();
        assert_eq!(songs[1].cover_art.as_deref(), Some(art_id.as_str()));
        assert_eq!(std::fs::read(art.0.join(model::art_file_name(&art_id))).unwrap(), PNG);
        assert_eq!(std::fs::read_dir(&art.0).unwrap().count(), 1);
        assert_eq!(index::embedded_lyrics(&conn, &first.id).unwrap().as_deref(), Some("la la la"));
        assert_eq!(first.replay_gain.as_ref().and_then(|g| g.track_gain), Some(-4.25));
        let path = index::file_path(&conn, &first.id).unwrap().unwrap();
        assert_eq!(read_picture(&path).unwrap(), PNG);
    }

    #[test]
    fn rescans_are_incremental() {
        let music = TempDir::new("rescan");
        write_song(&music.0, "a.wav", "A", "X", "Y", 1, None);
        let b = write_song(&music.0, "b.wav", "B", "X", "Y", 2, None);
        let mut conn = memory();
        let folder = index::add_folder(&conn, music.0.to_str().unwrap()).unwrap();
        let first = scan_folder(&mut conn, &folder, None, &|_, _| {}).unwrap();
        assert_eq!((first.found, first.read), (2, 2));

        // Nothing changed: nothing is read again.
        let again = scan_folder(&mut conn, &folder, None, &|_, _| {}).unwrap();
        assert_eq!((again.found, again.read, again.removed), (2, 0, 0));

        // A retag (size changes) is read; a deleted file leaves; a new one arrives.
        write_song(&music.0, "a.wav", "A (Remastered)", "X", "Y", 1, Some(PNG));
        std::fs::remove_file(&b).unwrap();
        write_song(&music.0, "sub/c.wav", "C", "X", "Y", 3, None);
        let id_a = model::song_id(&folder.id, "a.wav");
        index::set_starred(&conn, &id_a, true).unwrap();
        let last = scan_folder(&mut conn, &folder, None, &|_, _| {}).unwrap();
        assert_eq!((last.found, last.read, last.removed), (2, 2, 1));
        let songs = index::songs_page(&conn, "title", true, 0, 10).unwrap();
        assert_eq!(songs.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), vec!["A (Remastered)", "C"]);
        // Same id after the retag, and the favourite stayed.
        assert_eq!(songs[0].id, id_a);
        assert!(songs[0].starred.is_some());
    }

    #[test]
    fn untagged_files_use_their_folder_and_name() {
        let music = TempDir::new("untagged");
        std::fs::create_dir_all(music.0.join("Some Album")).unwrap();
        std::fs::write(music.0.join("Some Album/07 Track.wav"), wav_bytes()).unwrap();
        let mut conn = memory();
        let folder = index::add_folder(&conn, music.0.to_str().unwrap()).unwrap();
        scan_folder(&mut conn, &folder, None, &|_, _| {}).unwrap();
        let song = index::songs_page(&conn, "title", true, 0, 1).unwrap().remove(0);
        assert_eq!(song.title, "07 Track");
        assert_eq!(song.album.as_deref(), Some("Some Album"));
        assert_eq!(song.artist.as_deref(), Some(model::UNKNOWN_ARTIST));
    }
}
