---
version: 1
slug: "src-components-app-rs"
primary_target: "src/components/app.rs"
related_targets: ["src/components/views/home.rs","src/components/player/mod.rs"]
---

# Surface brief: RustySound app (all screens)

Scope: the whole app UI on iPhone (primary), Mac (primary), web (demo only). Mode: Operate.
Job: start something worth hearing in one tap; discovery leads, library one step behind, depth hidden in menus, sheets and the player.
Critique reference: /private/tmp/claude-501/-Users-arch-Projects-rust-rustysound/b615dd99-fbdd-448d-92d2-254516f1551f/scratchpad/mocks/stage/ (approved mock screens, code-led build).
Open decisions: off-server recommendation source beyond Navidrome (Last.fm key / ListenBrainz / iTunes previews).

## Direction contract

THESIS: The iTunes/iPod lineage grown into a full player. Artwork lives on a black stage; everything you operate sits in calm Apple grouped lists. Refuses the streaming wall of rows and the all-dark shell.
OWN-WORLD: Two materials. Stage: black with a radial lift (#26282c to #000). Page: #e6e7ea/#f8f8f9 light, #18191b/#222326 dark, hairlines #d6d8dc/#34363a. One blue, #2a7be4/#4a90ee; gloss only on progress tracks. System font, 17/15/13, tabular numerals. Inset lists (r12), 44px r4 thumbs, segmented controls, pills, dark floating player bar.
STORY: Open, see one recommendation worth playing on the stage, start it in one tap; depth waits in long-press menus, sheets and Now Playing.
FIRST VIEWPORT: iPhone: "Discover" large title on the stage, Cover Flow (centre ~55% width) with caption and reason, then For You/New/Rewind segments and mixes; dark mini player over the Discover/Library/Search tab bar. Mac: sidebar, stage header with Cover Flow, two list columns, black player bar.
FORM: Cover Flow lineage, my rank 1, user-pinned; seed c6f4c360.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
