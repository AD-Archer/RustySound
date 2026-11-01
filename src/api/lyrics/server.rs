//! The server provider: OpenSubsonic `getLyricsBySongId` on the song's own
//! server (lyrics embedded in its files or in sidecars next to them).
//!
//! Asks with `enhanced=true` when the server speaks `songLyrics` v2, and
//! keeps the word cues of the main vocal layer. Translations and
//! pronunciations are left out for now.

use super::{tidy_words, LyricLine, LyricWord, LyricsProvider, LyricsQuery, LyricsResult};
use crate::api::{
    server_lyrics_support, CueLinePayload, NavidromeClient, ServerLyricsSupport,
    StructuredLyricsPayload,
};

pub(super) async fn fetch_from_server(
    query: &LyricsQuery,
    timeout_seconds: u32,
) -> Result<Option<LyricsResult>, String> {
    let Some(target) = query.server_song.as_ref() else {
        return Ok(None);
    };
    let client = NavidromeClient::new(target.server.clone());
    let support = server_lyrics_support(&client.get_open_subsonic_extensions(timeout_seconds).await?);
    if support == ServerLyricsSupport::None {
        return Ok(None);
    }
    let entries = client
        .get_lyrics_by_song_id(
            &target.song_id,
            support == ServerLyricsSupport::Enhanced,
            timeout_seconds,
        )
        .await?;
    Ok(lyrics_from_structured(&entries))
}

fn is_main(entry: &StructuredLyricsPayload) -> bool {
    entry
        .kind
        .as_deref()
        .map(|kind| kind.trim().is_empty() || kind.eq_ignore_ascii_case("main"))
        .unwrap_or(true)
}

fn has_text(entry: &StructuredLyricsPayload) -> bool {
    entry.line.iter().any(|line| !line.value.trim().is_empty())
}

/// The lyrics from a `structuredLyrics` list: the main layer, synced when
/// there is a synced entry (one with word cues first), else unsynced. Pure;
/// used by tests.
pub(crate) fn lyrics_from_structured(entries: &[StructuredLyricsPayload]) -> Option<LyricsResult> {
    let main = entries
        .iter()
        .filter(|entry| is_main(entry) && has_text(entry))
        .collect::<Vec<_>>();
    let synced = main
        .iter()
        .filter(|entry| entry.synced)
        .max_by_key(|entry| !entry.cue_line.is_empty())
        .copied();
    let unsynced = main.iter().find(|entry| !entry.synced).copied();

    let synced_lines = synced.map(synced_lines_of).unwrap_or_default();
    let plain_source = unsynced.or(synced)?;
    let plain_lyrics = plain_source
        .line
        .iter()
        .map(|line| line.value.trim())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if plain_lyrics.is_empty() && synced_lines.is_empty() {
        return None;
    }
    Some(LyricsResult {
        provider: LyricsProvider::Server,
        plain_lyrics,
        synced_lines,
    })
}

fn synced_lines_of(entry: &StructuredLyricsPayload) -> Vec<LyricLine> {
    // A positive offset shows the lyrics sooner.
    let offset = entry.offset.unwrap_or(0.0) / 1000.0;
    let at = |ms: f64| (ms / 1000.0 - offset).max(0.0);
    let main_agents = entry
        .agents
        .iter()
        .filter(|agent| agent.role.eq_ignore_ascii_case("main"))
        .map(|agent| agent.id.as_str())
        .collect::<Vec<_>>();

    let mut lines = entry
        .line
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            let text = line.value.trim();
            if text.is_empty() {
                return None;
            }
            // Several cue lines can share an index (one per vocal agent);
            // the main agent's comes first, and is the one we light up.
            let cue_line = entry.cue_line.iter().find(|cue_line| {
                cue_line.index == index
                    && (main_agents.is_empty()
                        || cue_line
                            .agent_id
                            .as_deref()
                            .map(|id| main_agents.contains(&id))
                            .unwrap_or(false))
            });
            let words = cue_line
                .map(|cue_line| words_of(cue_line, &at))
                // Only when the cues spell out the whole line: a cue line for
                // one agent of a shared line would drop the other's words.
                .filter(|words| words.iter().map(|w| w.text.as_str()).collect::<String>() == text)
                .unwrap_or_default();
            Some(LyricLine {
                timestamp_seconds: at(line.start.unwrap_or(0.0)),
                text: text.to_string(),
                words,
            })
        })
        .collect::<Vec<_>>();
    lines.sort_by(|left, right| {
        left.timestamp_seconds
            .partial_cmp(&right.timestamp_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    lines
}

/// Word segments of a cue line. Each runs from its cue's first byte to the
/// next cue's, so the text between cues (spaces, untimed words) stays with
/// the word before it and the segments join back into the line. Falls back
/// to the cues' own text when the byte offsets don't fit the line.
fn words_of(cue_line: &CueLinePayload, at: &impl Fn(f64) -> f64) -> Vec<LyricWord> {
    let value = cue_line.value.as_str();
    let starts = cue_line
        .cue
        .iter()
        .map(|cue| cue.byte_start)
        .collect::<Option<Vec<_>>>()
        .filter(|starts| {
            starts.windows(2).all(|pair| pair[0] <= pair[1])
                && starts
                    .iter()
                    .all(|start| *start <= value.len() && value.is_char_boundary(*start))
        });

    let texts: Vec<String> = match starts {
        Some(starts) => starts
            .iter()
            .enumerate()
            .map(|(i, start)| {
                let from = if i == 0 { 0 } else { *start };
                let to = starts.get(i + 1).copied().unwrap_or(value.len());
                value[from..to].to_string()
            })
            .collect(),
        None => cue_line.cue.iter().map(|cue| cue.value.clone()).collect(),
    };

    tidy_words(
        cue_line
            .cue
            .iter()
            .zip(texts)
            .map(|(cue, text)| LyricWord {
                start_seconds: at(cue.start),
                end_seconds: cue.end.map(|end| at(end)),
                text,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{OpenSubsonicExtension, SubsonicResponse};

    fn parse(json: &str) -> Vec<StructuredLyricsPayload> {
        let response: SubsonicResponse = serde_json::from_str(json).unwrap();
        response.subsonic_response.lyrics_list.unwrap().structured_lyrics
    }

    #[test]
    fn songlyrics_v2_means_enhanced() {
        let ext = |name: &str, versions: Vec<u32>| OpenSubsonicExtension {
            name: name.to_string(),
            versions,
        };
        assert_eq!(server_lyrics_support(&[]), ServerLyricsSupport::None);
        assert_eq!(
            server_lyrics_support(&[ext("formPost", vec![1])]),
            ServerLyricsSupport::None
        );
        assert_eq!(
            server_lyrics_support(&[ext("songLyrics", vec![1])]),
            ServerLyricsSupport::Lines
        );
        assert_eq!(
            server_lyrics_support(&[ext("songLyrics", vec![1, 2])]),
            ServerLyricsSupport::Enhanced
        );
    }

    #[test]
    fn v1_synced_lines_apply_offset_and_keep_plain() {
        let entries = parse(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":[
              {"lang":"eng","offset":-100,"synced":true,"line":[
                {"start":0,"value":"It's bugging me"},{"start":2000,"value":"Grating me"}]},
              {"lang":"und","synced":false,"line":[{"value":"It's bugging me"},{"value":"Grating me"}]}
            ]}}}"#,
        );
        let lyrics = lyrics_from_structured(&entries).unwrap();
        assert_eq!(lyrics.provider, LyricsProvider::Server);
        assert_eq!(lyrics.plain_lyrics, "It's bugging me\nGrating me");
        assert_eq!(lyrics.synced_lines.len(), 2);
        // A negative offset shows lines later.
        assert!((lyrics.synced_lines[0].timestamp_seconds - 0.1).abs() < 1e-9);
        assert!((lyrics.synced_lines[1].timestamp_seconds - 2.1).abs() < 1e-9);
        assert!(lyrics.synced_lines.iter().all(|line| line.words.is_empty()));
    }

    #[test]
    fn v2_cues_become_words_by_byte_offset() {
        let entries = parse(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":[
              {"kind":"main","lang":"ko","synced":true,
               "line":[{"start":2747,"value":"눈을 뜬 순간"}],
               "cueLine":[{"index":0,"start":2747,"end":6214,"value":"눈을 뜬 순간","cue":[
                 {"start":2747,"end":3018,"value":"눈","byteStart":0,"byteEnd":2},
                 {"start":3018,"end":3179,"value":"을","byteStart":3,"byteEnd":5},
                 {"start":3179,"end":3582,"value":" ","byteStart":6,"byteEnd":6},
                 {"start":3582,"end":4100,"value":"뜬","byteStart":7,"byteEnd":9},
                 {"start":4100,"end":4500,"value":" ","byteStart":10,"byteEnd":10},
                 {"start":4500,"end":5200,"value":"순","byteStart":11,"byteEnd":13},
                 {"start":5200,"end":6214,"value":"간","byteStart":14,"byteEnd":16}]}]},
              {"kind":"translation","lang":"eng","synced":true,
               "line":[{"start":2747,"value":"The moment I opened my eyes"}]}
            ]}}}"#,
        );
        let lyrics = lyrics_from_structured(&entries).unwrap();
        assert_eq!(lyrics.plain_lyrics, "눈을 뜬 순간");
        let words = &lyrics.synced_lines[0].words;
        let texts = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>();
        // Space cues fold into the syllable before them.
        assert_eq!(texts, ["눈", "을 ", "뜬 ", "순", "간"]);
        assert!((words[0].start_seconds - 2.747).abs() < 1e-9);
        assert_eq!(words[4].end_seconds, Some(6.214));
    }

    #[test]
    fn untimed_text_between_cues_stays_with_the_word_before() {
        let entries = parse(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":[
              {"synced":true,"line":[{"start":0,"value":"Oh love love me tonight"}],
               "cueLine":[{"index":0,"value":"Oh love love me tonight","cue":[
                 {"start":0,"value":"Oh","byteStart":0,"byteEnd":1},
                 {"start":900,"value":"love","byteStart":8,"byteEnd":11},
                 {"start":1300,"value":"me","byteStart":13,"byteEnd":14},
                 {"start":1600,"value":"tonight","byteStart":16,"byteEnd":22}]}]}
            ]}}}"#,
        );
        let lyrics = lyrics_from_structured(&entries).unwrap();
        let texts = lyrics.synced_lines[0]
            .words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(texts, ["Oh love ", "love ", "me ", "tonight"]);
    }

    #[test]
    fn agent_cue_lines_that_split_a_line_are_not_used() {
        let entries = parse(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":[
              {"synced":true,"line":[{"start":1000,"value":"Hello echo"}],
               "agents":[{"id":"lead","role":"main"},{"id":"backing","role":"bg"}],
               "cueLine":[
                 {"index":0,"agentId":"lead","value":"Hello","cue":[
                   {"start":1000,"end":1400,"value":"He","byteStart":0,"byteEnd":1},
                   {"start":1400,"end":1800,"value":"llo","byteStart":2,"byteEnd":4}]},
                 {"index":0,"agentId":"backing","value":"echo","cue":[
                   {"start":2000,"end":2500,"value":"echo","byteStart":0,"byteEnd":3}]}]}
            ]}}}"#,
        );
        let lyrics = lyrics_from_structured(&entries).unwrap();
        assert_eq!(lyrics.synced_lines[0].text, "Hello echo");
        assert!(lyrics.synced_lines[0].words.is_empty());
    }

    #[test]
    fn empty_list_is_no_lyrics() {
        let entries = parse(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":[]}}}"#,
        );
        assert!(lyrics_from_structured(&entries).is_none());
        let blank = parse(
            r#"{"subsonic-response":{"status":"ok","lyricsList":{"structuredLyrics":[
              {"synced":false,"line":[{"value":"  "}]}]}}}"#,
        );
        assert!(lyrics_from_structured(&blank).is_none());
    }
}
