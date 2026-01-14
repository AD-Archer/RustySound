use dioxus::prelude::*;

/// A simple image component that displays images directly.
/// Caching functionality has been simplified to avoid hook conflicts.
#[component]
pub fn CachedImage(
    src: String,
    alt: String,
    class: String,
    #[props(default = String::new())] cache_key: String,
    #[props(default = 24)] expiry_hours: u32,
) -> Element {
    // Suppress unused variable warnings
    let _ = cache_key;
    let _ = expiry_hours;

    rsx! {
        img {
            src: "{src}",
            alt: "{alt}",
            class: "{class}",
            loading: "lazy",
        }
    }
}