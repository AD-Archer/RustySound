# Now Playing widgets (iPhone and Mac)

RustySound can ship WidgetKit widgets that show the current song (cover,
title, artist, progress) with Play/Pause and Next buttons:

| Where | Sizes |
| --- | --- |
| iPhone Home Screen, StandBy | small, medium |
| iPhone Lock Screen | rectangular (song + progress), circular (play/pause ringed by progress), inline |
| Mac desktop and Notification Center | small, medium |

Tapping the widget (outside the buttons) opens the app. The buttons need
iOS 17 or macOS 14. The widget is **off by default**: normal builds and
releases don't change until you turn it on with `RS_WIDGETS=1`.

## How it works

```
 RustySound (Rust)                     App Group container                 Widget extension (Swift)
 ─────────────────                     ───────────────────                 ────────────────────────
 lock-screen Now Playing (iPhone)  ──▶  now-playing.json  ──────────────▶  timeline: draws progress
 menu bar signals (Mac)                 cover-<id>.img                     from position + updated_at
   src/now_playing_widget.rs            (written on changes only,
   reloadAllTimelines via the           then WidgetKit reloads)
   bridge framework

 lock screen / menu bar handlers  ◀──  command.json  ◀── Darwin notification ── Play/Pause, Next intents
```

- **App to widget.** The app writes a small JSON snapshot when the song, play
  state, speed or cover changes, or after a seek (at most every 5 seconds for
  seeks). Plain progress is never written: the widget's progress bar and
  clock run by themselves from `position` + `updated_at`.
- **Widget to app.** The buttons are App Intents that run in the widget's
  process. They write `command.json` and post the Darwin notification
  `app.adarcher.rustysound.widget.command`. The app (alive in the background
  while it plays) runs the same handlers as the Lock Screen (iPhone) or the
  menu bar item (Mac). The intent waits up to 1.5 s for the app's new
  snapshot, so the widget redraws with the real state.
- **A paused iPhone app gets suspended** and can't hear the notification. After
  20 seconds paused, the widget's Play button becomes "open the app and play"
  (`openAppWhenRun`). The app plays the waiting command when it becomes
  active. Commands older than 15 s are dropped.
- The app is a Rust binary with no Swift of its own, and WidgetKit is
  Swift-only. So the app loads `RustySoundWidgetBridge.framework` (embedded
  with the widget) with `dlopen`. If the framework or the App Group
  entitlement is missing, the app skips all widget work.

The code: `apple/widgets/` (Xcode project, Swift sources, `build.sh`,
`embed.sh`), `src/now_playing_widget.rs` (snapshot, throttling, commands, Mac
hook), and small hooks in `src/components/audio_manager/native_ios_*.rs`.

## What you need to create in the Apple Developer portal

Do this once, at developer.apple.com → Certificates, Identifiers & Profiles.
The ids below are the defaults. If you pick others, pass them as
`RS_APP_BUNDLE_ID` / `RS_APP_GROUP` (see below).

1. **App Group** (Identifiers → App Groups → +):
   `group.app.adarcher.rustysound`
2. **App IDs** (Identifiers → App IDs → +, type App). Explicit App IDs work
   for both iOS and macOS.
   - `app.adarcher.rustysound`: the app. If it already exists, edit it. Turn
     on **App Groups** and assign `group.app.adarcher.rustysound`.
   - `app.adarcher.rustysound.widgets`: the widget extension (new). Turn on
     **App Groups** and assign the same group.
3. **Provisioning profiles** (Profiles → +), one per App ID per use:
   - iPhone on your devices: *iOS App Development* (or *Ad Hoc*) for both App
     IDs.
   - App Store / TestFlight: *App Store Connect* for both App IDs.
   - Mac outside the App Store: *Developer ID* (macOS) for both App IDs. macOS
     15 needs a profile to grant a `group.`-style App Group.
   Download the `.mobileprovision` / `.provisionprofile` files.
4. **Entitlements**: nothing to write by hand. `apple/widgets/embed.sh` takes
   them from the profiles and makes sure each bundle has:
   - app and widget: `com.apple.security.application-groups =
     [group.app.adarcher.rustysound]`
   - Mac widget only: `com.apple.security.app-sandbox = true` (every macOS
     extension must be sandboxed)

## Turning it on

```sh
# Simulator: everything ad-hoc signed; installable with `xcrun simctl install`.
just bundle-sim-widgets

# Signed for your devices / Developer ID.
RS_WIDGETS=1 \
RS_IOS_CODESIGN_IDENTITY="Apple Development: Your Name (TEAMID)" \
RS_IOS_APP_PROFILE=~/profiles/RustySound_iOS.mobileprovision \
RS_IOS_WIDGETS_PROFILE=~/profiles/RustySound_Widgets_iOS.mobileprovision \
RS_MACOS_CODESIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" \
RS_MACOS_APP_PROFILE=~/profiles/RustySound_macOS.provisionprofile \
RS_MACOS_WIDGETS_PROFILE=~/profiles/RustySound_Widgets_macOS.provisionprofile \
./scripts/bundle-apple.sh
```

| Variable | Default | Meaning |
| --- | --- | --- |
| `RS_WIDGETS` | `0` | `1` builds and embeds the widget (`just bundle-apple-widgets`, `just bundle-sim-widgets`). |
| `RS_APP_GROUP` | `group.app.adarcher.rustysound` | App Group id. Written to `RSAppGroup` in both Info.plists. |
| `RS_APP_BUNDLE_ID` | `app.adarcher.rustysound` | The widget is `<id>.widgets`, the bridge `<id>.widgetbridge`. |
| `RS_IOS_CODESIGN_IDENTITY` | none | Unset: the iOS app stays unsigned (the current unsigned-IPA path). |
| `RS_IOS_APP_PROFILE`, `RS_IOS_WIDGETS_PROFILE` | none | iOS profiles for the app and the widget. |
| `RS_MACOS_CODESIGN_IDENTITY` | `-` (ad-hoc) | A Developer ID identity also adds the hardened runtime and a timestamp. |
| `RS_MACOS_APP_PROFILE`, `RS_MACOS_WIDGETS_PROFILE` | none | macOS profiles for the app and the widget. |

### Release builds (CI)

`.github/workflows/build.yml` doesn't turn the widget on. Before you do,
change two things there:

- Pass `RS_WIDGETS=1`, the identities and the profiles to the *Build macOS +
  iOS bundles* step. Decode the profiles from secrets like the certificate.
- In *Sign macOS app*, don't re-sign with `codesign --force --deep`. A deep
  re-sign drops the widget's sandbox and App Group entitlements, and macOS
  then won't load the widget. `embed.sh` already signs inside out with the
  Developer ID (hardened runtime, timestamp), so go straight to notarizing.

### Sideloaded (unsigned) IPAs

With `RS_WIDGETS=1` and no `RS_IOS_CODESIGN_IDENTITY`, the IPA contains the
widget unsigned. Tools that re-sign with a free account (AltStore,
Sideloadly) often rename App Groups. The app and the widget then can't find
each other: the widget says "Not Playing" and its buttons do nothing. Signing
with your own profiles avoids that.

## Limits

- **Mac app not running**: the snapshot stays as it was (a song that was
  playing shows as stopped once it would have ended). Play/Next do nothing
  until the app is open. Tapping the widget opens it.
- **Cold start from Play** (iPhone app killed, not just suspended): the app
  opens and gets the play command, but if nothing is loaded yet, it just
  opens.
- **Playback speed other than 1×**: the medium widget's elapsed time shows
  the snapshot's value instead of a running clock.

## Testing in the Simulator

```sh
just bundle-sim-widgets        # or the iOS half: dx bundle --ios --target aarch64-apple-ios-sim
xcrun simctl boot "iPhone 17"
xcrun simctl install booted dist/apple/ios/Rustysound.app
xcrun simctl spawn booted pluginkit -m -p com.apple.widgetkit-extension   # lists app.adarcher.rustysound.widgets
xcrun simctl launch --console-pty booted app.adarcher.rustysound          # prints "[widgets] bridge loaded, container: Some(...)"
xcrun simctl get_app_container booted app.adarcher.rustysound groups      # the container with now-playing.json
```

The simulator ignores signature entitlements. Simulator builds therefore link
the App Group entitlement into the binaries as a `__TEXT,__entitlements`
section, as Xcode does (`apple/widgets/sim-entitlements.sh`).

To try a button without the widget, write a command and post the
notification:

```sh
G=$(xcrun simctl get_app_container booted app.adarcher.rustysound groups | awk '{print $2}')
printf '{"action":"toggle","at":%s,"id":"t1"}' "$(date +%s)" > "$G/command.json"
xcrun simctl spawn booted notifyutil -p app.adarcher.rustysound.widget.command
```

To add the widget, long-press the Home Screen, tap Edit → Add Widget and
search for RustySound.
