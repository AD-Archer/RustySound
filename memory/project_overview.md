---
name: RustySound project overview
description: Tech stack, architecture, and key conventions for RustySound
type: project
---

RustySound is a cross-platform Navidrome/Subsonic music client written in Rust using the Dioxus framework.

**Tech stack:** Rust 2021, Dioxus 0.7, token-based CSS (no Tailwind), SQLite (rusqlite), async/Tokio, WASM for web target.

**Platforms:** Web (WASM), Desktop (macOS/Windows/Linux via WebView), iOS/Android.

**CSS loading:** `src/styles.rs` is the one registry. Desktop and iOS inline CSS via `include_str!()`; web loads `asset!()` stylesheet links. Both go through the `GlobalStyles` component. Load order: tokens.css → base.css → components.css → views/*.css.

**State:** Dioxus signals + context providers (`AppSettings`, `now_playing`, `queue`, `servers`, etc.) initialized in `AppShell` (`src/components/app.rs`).

**Settings persistence:** `AppSettings` struct in `src/db/mod.rs`, saved to SQLite (native) or localStorage (web) via `save_settings()`.

**Routing:** `AppView` enum in `src/components/app_view.rs`.

**Key CSS classes:** the `rs-` UI kit in `assets/styling/components.css` (`.rs-view`, `.rs-list`, `.rs-row`, `.rs-btn`, `.rs-menu`, …); every colour comes from `tokens.css`.
