---
name: RustySound project overview
description: Tech stack, architecture, and key conventions for RustySound
type: project
---

RustySound is a cross-platform Navidrome/Subsonic music client written in Rust using the Dioxus framework.

**Tech stack:** Rust 2021, Dioxus 0.7.3, Tailwind CSS (compiled), SQLite (rusqlite), async/Tokio, WASM for web target.

**Platforms:** Web (WASM), Desktop (macOS/Windows/Linux via WebView), iOS/Android.

**CSS loading:** Desktop inlines CSS via `include_str!()` in `src/main.rs`; web loads via `asset!()` stylesheet links. Both go through `GlobalStyles` component. Load order: tailwind.css → themes.css → app.css.

**State:** Dioxus signals + context providers (`AppSettings`, `now_playing`, `queue`, `servers`, etc.) initialized in `AppShell` (`src/components/app.rs`).

**Settings persistence:** `AppSettings` struct in `src/db/mod.rs`, saved to SQLite (native) or localStorage (web) via `save_settings()`.

**Routing:** `AppView` enum in `src/components/app_view.rs`.

**Key custom CSS classes:** `.app-container`, `.glass`, `.page-shell`, `.player-shell`, `.sidebar-shell` — all in `assets/styling/app.css` using `--rs-*` CSS variables.
