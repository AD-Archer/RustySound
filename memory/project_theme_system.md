---
name: Theme system architecture
description: How the CSS variable theming framework works and how to add new themes
type: project
---

RustySound has a full theming system as of 2026-04-02.

**Core files:**
- `assets/styling/themes.css` — theme import entrypoint (ordered `@import`s)
- `assets/styling/themes/shared/base.css` — global variable-driven rules, dynamic accent mapping, and shared layout/chrome baselines
- `assets/styling/themes/<theme>/theme.css` — one folder per theme (`rusty`, `spot`, `fruit`, `navi`, `y2k`, `aero`, `aqua`, `material`, `fluent`, `hig`)
- `assets/styling/app.css` — custom classes now use `var(--rs-*)` tokens
- `src/main.rs` — inlines the split theme files for desktop, and loads `themes.css` (the import entrypoint) for web; both paths apply theme styles before `app.css` in `GlobalStyles`
- `src/components/app.rs` — sets `data-theme="{active_theme}"` attribute on the root `.app-container` div
- `src/components/views/settings.rs` — "Appearance" tab with `ThemeCard` component for visual picking
- `src/db/mod.rs` — `AppSettings.theme` field (default: `"rusty"`)

**Core themes:** `rusty` (default, emerald), `spot` (Spotify-green), `fruit` (Apple Music red), `navi` (deep navy, sky-blue)

**Experimental themes:** `y2k` (hot magenta, monospace, neon), `aero` (Frutiger Aero, ocean cyan), `aqua` (macOS Aqua, pinstripe), `material` (Material You, purple), `fluent` (Windows 11, Mica), `hig` (Apple HIG, OLED black)

**Custom CSS:** `AppSettings.custom_css` — injected live via `use_effect` in `app.rs` using `document::eval`. Editable in Settings → Appearance tab.

**How to create a new theme:**
1. Create `assets/styling/themes/your-name/theme.css`
2. Add your `[data-theme="your-name"]` variable block and any per-theme utility/structure/chrome overrides there
3. Add an `@import` entry for your file in `assets/styling/themes.css`
4. Add a `ThemeCard` entry in the Appearance tab grid in `settings.rs`
5. Done — no Rust recompile needed for CSS-only changes in web mode; desktop needs rebuild

**Why:** `data-theme` on the root container scopes all `[data-theme="x"] .tw-class` overrides without touching component code. CSS variables propagate through all descendants automatically.

**How to apply:** `app_settings().theme` drives it; changing the signal updates the DOM attribute reactively.
