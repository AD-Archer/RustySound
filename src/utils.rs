/// Utility helpers for RustySound

/// Create a simple slug from a string suitable for URLs.
/// Lowercases the string, converts groups of non-alphanumeric chars to single hyphens,
/// and trims leading/trailing hyphens.
pub fn slugify<S: AsRef<str>>(s: S) -> String {
    let s = s.as_ref().to_lowercase();
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;

    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else {
            if !prev_dash {
                out.push('-');
                prev_dash = true;
            }
        }
    }

    out.trim_matches('-').to_string()
}
