# RustySound TV wrapper

For Android TV sets that can't run the native app: Dioxus 0.7 builds 64-bit Android only, and many TVs run 32-bit. This is a small Java app that opens the RustySound **web build** full screen and gives it the same `window.RustySoundAndroid` bridge as the native shell, so TV mode, D-pad navigation and Back all work.

- The web build is served from the home server over Tailscale (`http://100.126.39.68:4748/`, `services/rustysound-web`). Updating the app = redeploying the web build; no reinstall.
- Build (server, Android dev image): `gradle -p android/tv-wrapper assembleDebug [-PwebUrl=http://host:port/]`
- Install: `adb connect <tv>:5555 && adb install -r android/tv-wrapper/app/build/outputs/apk/debug/app-debug.apk`
