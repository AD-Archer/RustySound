//! Recent searches, kept on this device only: localStorage on the web, a small
//! JSON file in the app data folder on native builds.

const MAX_RECENT: usize = 10;
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "rustysound.recent_searches";
#[cfg(not(target_arch = "wasm32"))]
const FILE_NAME: &str = "recent_searches.json";

/// Puts `query` first, without duplicates (case-insensitive), capped at 10.
pub(super) fn remember(list: &[String], query: &str) -> Vec<String> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return list.to_vec();
    }
    let mut next = vec![query.to_string()];
    next.extend(
        list.iter()
            .filter(|item| !item.eq_ignore_ascii_case(query))
            .take(MAX_RECENT - 1)
            .cloned(),
    );
    next
}

fn parse(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|item| !item.trim().is_empty())
        .take(MAX_RECENT)
        .collect()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn load() -> Vec<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
        .map(|raw| parse(&raw))
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn save(list: &[String]) {
    if let (Some(storage), Ok(raw)) = (
        web_sys::window().and_then(|w| w.local_storage().ok().flatten()),
        serde_json::to_string(list),
    ) {
        let _ = storage.set_item(STORAGE_KEY, &raw);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn load() -> Vec<String> {
    crate::storage::app_data_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join(FILE_NAME)).ok())
        .map(|raw| parse(&raw))
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn save(list: &[String]) {
    if let (Some(dir), Ok(raw)) = (crate::storage::app_data_dir(), serde_json::to_string(list)) {
        let _ = std::fs::write(dir.join(FILE_NAME), raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_duplicates() {
        let list = vec!["Paper Comets".to_string(), "jazz".to_string()];
        assert_eq!(remember(&list, "JAZZ"), vec!["JAZZ", "Paper Comets"]);
        assert_eq!(remember(&list, "x"), list);
        let long: Vec<String> = (0..12).map(|i| format!("q{i}")).collect();
        assert_eq!(remember(&long, "new").len(), MAX_RECENT);
    }

    #[test]
    fn parses_garbage_as_empty() {
        assert!(parse("not json").is_empty());
        assert_eq!(parse(r#"["a", " ", "b"]"#), vec!["a", "b"]);
    }
}
