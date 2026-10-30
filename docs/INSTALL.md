# Installing and updating RustySound

The [README](../README.md) has the one-line installs. This page covers updates and fixes.

## Mac

**Update**

```bash
brew upgrade --cask rustysound && xattr -dr com.apple.quarantine /Applications/Rustysound.app
```

If you installed the DMG, download it again and replace the app.

**"Rustysound is damaged and can't be opened"**

RustySound isn't signed with an Apple Developer ID, so macOS blocks it after a download. Run:

```bash
xattr -dr com.apple.quarantine /Applications/Rustysound.app
```

This removes the "downloaded from the internet" flag from the app. Do it after every install or update. If you'd rather not use the terminal, right-click the app, choose **Open**, then **Open Anyway** in System Settings → Privacy & Security.

**Homebrew says the tap is untrusted**

Homebrew 6 and later only load taps you trust, which is what `brew trust ad-archer/tap` does. To trust just this app instead of the whole tap, use `brew trust --cask ad-archer/tap/rustysound`.

## iPhone

The AltStore link adds the RustySound source, so updates show up in AltStore. You can also add the source by hand: `https://ad-archer.github.io/packages/source.json`, in [AltStore](https://altstore.io/) or [LiveContainer](https://github.com/LiveContainer/LiveContainer).

To sideload instead, download [Rustysound-unsigned.ipa](https://github.com/AD-Archer/RustySound/releases/latest/download/Rustysound-unsigned.ipa) and install it with AltStore, [Sideloadly](https://sideloadly.io/) or Xcode.

## Windows

**Update:** `scoop update rustysound`, or run the newest installer.

Windows may warn that the installer is unsigned. Builds from this repository are safe to run.

## Linux

**Update:** `flatpak update --user app.adarcher.rustysound`

**Remove:** `flatpak uninstall --user app.adarcher.rustysound && flatpak remote-delete --user adarcher-rustysound`

**"requires the runtime org.gnome.Platform … which was not found"**

```bash
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.gnome.Platform//49
```

Then run the install command again. For a system-wide install, leave out `--user` everywhere.

## Android

Download [the .apk](https://github.com/AD-Archer/RustySound/releases/latest/download/app-debug.apk), allow installs from unknown sources when Android asks, and open it. Android works, but gets less attention than the other platforms.

## Docker

The [`docker-compose.yml`](../docker-compose.yml) in this repo does the same as the `docker run` line: `docker compose up -d`, and `docker compose down` to stop.

**Update:** `docker pull ghcr.io/ad-archer/rustysound:latest`, then recreate the container.
