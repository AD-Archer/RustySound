# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

Apple first: iOS (iPhone) and macOS desktop are the products that matter and get tuned and tested. One Dioxus 0.7 codebase renders in a webview on every target, so "native" means following Apple's conventions per form factor (iPhone tab bar and sheets; Mac sidebar, keyboard, hover) and using the OS media integrations (Now Playing, lock screen and remote controls), not native widgets. Web/PWA is a demo surface only. Windows, Linux and Android must keep building and working but are not design targets.

## Users

One person (the author) and people like them: self-hosters who run their own Navidrome or Subsonic-compatible server and own their library. They listen on iPhone on the move and on a Mac at a desk, often with the app in the background or on the lock screen.

## Product Purpose

A small (< 15 MB), fast client for your own music server that also helps you find what to listen to next. Success: starting music is one tap, the app never feels sluggish, and it keeps surfacing things you will like, both from your library and from outside it.

## Positioning

Your own library, with the discovery of a streaming service. RustySound leads with discovery: recommendations shaped by your listening (Last.fm-style similarity), generated playlists built from them, new music from outside your server worth adding, and resurfacing of classics and albums you haven't played in a while. All of this covers both songs and albums.

## Operating Context

- Connects to one or more Navidrome/Subsonic servers; streams, scrobbles, stars, rates, and manages playlists on the server.
- Offline: downloads songs and albums for offline playback, caches art.
- Plays in the background; controlled from the lock screen, Control Center, and media keys.
- Synced lyrics, a queue, song details and related-songs panels around the now-playing view.

## Capabilities and Constraints

- Stack: Rust + Dioxus 0.7 (web, desktop, mobile features). Platform audio backends: web audio element, iOS native backend with remote controls, Windows SMTC.
- Existing areas: Home, Search, Albums, Artists, Songs, Playlists, Favorites, Radio, Random, Bookmarks, Queue, Downloads, Stats, Settings; album, artist and playlist detail; add-to menu; song details overlay with lyrics, queue, and related panels.
- Themes: the redesign keeps a few curated themes built on shared design tokens (replacing today's 11 class-override themes).
- Discovery data: Navidrome exposes similar songs, artist info with similar artists (including ones not in the library), and top songs, sourced from the server's Last.fm/Spotify agents. Off-server recommendations need an external source (Last.fm or ListenBrainz). Open decision: which source, and whether the user supplies an API key.

## Brand Commitments

- Name: RustySound.
- The user named /Users/arch/Projects/sideprojects/ipod-navi-down as an interface they like as it stands: Apple-native, quiet, Cover Flow on a black stage, inset grouped lists, one accent.
- Wants modern, sleek and beautiful, with a lot of capability that stays out of the way: not cluttered, intuitive, good to the touch.

## Evidence on Hand

- Real album art and metadata come from the user's server at runtime; mockups must use clearly fake placeholder content.
- No user counts, benchmarks or testimonials exist; don't invent any.

## Product Principles

1. Playing music is never more than one tap away.
2. Lead with discovery; keep the library one step behind it.
3. Depth is hidden, not removed: rarely used power lives in menus, sheets and settings.
4. Feel native on iPhone and Mac; speed is part of the design.
5. Reusable components over one-off screens.

## Accessibility & Inclusion

Follow Apple platform norms: 44 pt touch targets, honor Reduce Motion, readable contrast in light and dark, full keyboard use on Mac.
