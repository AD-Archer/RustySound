---
name: Theme system architecture
description: How themes and appearance work (token CSS, no Tailwind) and how to add a theme
type: project
---

RustySound styles everything with token-based CSS. Tailwind, `app.css` and the 11 class-override themes were removed in the Stage redesign (2026-10).

**Core files:**
- `assets/styling/tokens.css`: every colour, radius, size and motion token. Themes are token sets selected by `[data-theme="stage" | "graphite" | "tape"]`; appearance by `[data-appearance="system" | "light" | "dark"]`.
- `assets/styling/base.css`: reset, typography, focus ring, scrollbars, platform tweaks.
- `assets/styling/components.css`: the UI kit (`src/components/ui/**`), classes prefixed `rs-`.
- `assets/styling/views/*.css`: one file per view area.
- `src/styles.rs`: the single stylesheet registry (`include_str!` on desktop/iOS, `asset!` links on web).
- `src/components/ui/env.rs`: `stage_theme()` maps stored theme names, including the retired ones, to a Stage theme at read time.

**Custom CSS:** `AppSettings.custom_css`, injected live from `app.rs`. Editable in Settings → Appearance.

**How to add a theme:**
1. Add a `[data-theme="name"]` block (light) and dark blocks (explicit `[data-appearance="dark"]` and the `prefers-color-scheme` copy) in `tokens.css`. Keep text tokens at 4.5:1 or better.
2. Map the name in `stage_theme()` and add it to the theme picker in Settings → Appearance.
