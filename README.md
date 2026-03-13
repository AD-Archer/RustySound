# RustySound

A cross-platform music streaming client for Navidrome and Subsonic-compatible servers, built with Rust and Dioxus.

![RustySound desktop screenshot](https://www.antonioarcher.com/images/projects/rustysound/desktop/sound_menu.webp)
<img alt="album page art" src="https://github.com/user-attachments/assets/06267508-388f-456b-a2e3-ae45dc00f11c" />
<img alt="album" src="https://github.com/user-attachments/assets/bc93c22e-bcf1-4a41-9d3c-fdba82a36214" />


# Features

- 🎵 **Multi-platform Support**: Available on Desktop (macOS, Windows, Linux), Mobile (iOS, Android), and Web
- 🎧 **Audio Playback**: High-quality audio streaming with queue management
- 📱 **Server Integration**: Connect to Navidrome and Subsonic-compatible music servers
- 💾 **Local Storage**: Persistent settings and playback state across sessions
- 🎼 **Playlist Management**: Create and manage playlists
- 🔍 **Search & Browse**: Browse your music library by artists, albums, and tracks
- 🎚️ **Audio Controls**: Play, pause, skip, shuffle, and repeat functionality
- 🌙 **Modern UI**: Clean, responsive interface built with Tailwind CSS

# Supported Platforms

## Web

- **Browser**: WebAssembly-based web application
- **Progressive Web App**: Installable PWA support


## Desktop 

- **macOS**: DMG installer and Homebrew
- **Windows**: NSIS installer and scoop.
- **Linux**: Flatpak

## Mobile

- **iOS**: unsigned .ipa 
- **Android**: APK release artifact

> Android status: Android builds are published and kept feature-aligned, but Android is not under active day-to-day development and may contain platform-specific bugs.


# Installation

## Desktop

### IOS
1. Add `https://ad-archer.github.io/packages/source.json` as a source in [AltStore/AltServer](https://altstore.io/) or [LiveContainer](https://github.com/LiveContainer/LiveContainer)
2. Install RustySound from that source on your device
3. Use the latest `.ipa` from [Releases](https://github.com/AD-Archer/RustySound/releases) only if you want to sideload manually instead of using the source feed

### Android
1. Download the latest `.apk` file from [Releases](https://github.com/AD-Archer/RustySound/releases)
2. Enable installation from unknown sources on your device
3. Install the APK and launch RustySound


### macOS

#### Homebrew
```bash
brew tap ad-archer/tap
brew install --cask rustysound
```

To update:

```bash
brew upgrade rustysound
```


#### DMG

1. Download the latest `.dmg` file from [Releases](https://github.com/AD-Archer/RustySound/releases)
2. Open the DMG and drag RustySound to your Applications folder

If macOS says the app is damaged or won't open:

1. In Finder, right-click `RustySound.app` and choose **Open**, then confirm.
2. Or run:

```bash
xattr -dr com.apple.quarantine /Applications/RustySound.app
open /Applications/RustySound.app
```

### Windows

#### Scoop

```powershell
scoop bucket add ad-archer https://github.com/ad-archer/scoop
scoop install ad-archer/rustysound
```


#### Executable
1. Download the latest `.exe` file from [Releases](https://github.com/AD-Archer/RustySound/releases)
2. Open the exe and install RustySound

Note: Antivirus may flag this installer since this exe is not verified by windows.

Note: release artifacts may be unsigned/ad-hoc signed when Apple notarization secrets are not configured in CI. For public distribution without warnings, a paid Apple Developer signing + notarization flow is required.

### Linux

#### Flatpak (Recommended)

```bash
flatpak remote-add --if-not-exists --user adarcher-rustysound https://ad-archer.github.io/packages/rustysound.flatpakrepo
flatpak install --user adarcher-rustysound app.adarcher.rustysound
flatpak run app.adarcher.rustysound
```

To remove:

```bash
flatpak uninstall --user app.adarcher.rustysound
flatpak remote-delete --user adarcher-rustysound
```

### Web

Visit [rustysound](https://rustysound-demo.adarcher.app) to use the web version.

#### Docker Deployment

You can also run RustySound as a Docker container:

1. Ensure you have Docker and Docker Compose installed
2. Clone this repository or copy [`docker-compose.yml`](https://raw.githubusercontent.com/AD-Archer/RustySound/refs/heads/main/docker-compose.yml)
3. Run the application:

```bash
docker-compose up -d
```

The web interface will be available at `http://localhost:8080`.

To stop the container:

```bash
docker-compose down
```

#### Manual Docker Run

If you prefer to run the container directly:

```bash
docker run -d -p 8080:80 --name rustysound ghcr.io/ad-archer/rustysound:latest
```

### Prerequisites

- Rust 1.70+ ([install here](https://rustup.rs/))
- Dioxus CLI: `curl -sSL https://dioxus.dev/install.sh | sh`

### Setup

1. Clone the repository:

```bash
git clone https://github.com/AD-Archer/RustySound.git
cd RustySound
```

2. Install dependencies:

```bash
cargo build
```

### Running the Application

#### Development Server

```bash
dx serve
```

#### Just Shortcuts

```bash
just              # list recipes
just serve        # dx serve
just serve-ios    # iOS simulator dev (safe linker env)
just serve-android # Android dev/debug (auto-create/start emulator)
just bundle       # macOS + iOS + unsigned IPA
just bundle-android-release # Android release APK into dist/android
just check        # cargo check
```

#### iOS Simulator Development

Use the helper below instead of raw `dx serve --ios` if your shell exports Homebrew/Nix compiler flags:

```bash
./scripts/serve-ios.sh
```

You can pass normal `dx serve` options through:

```bash
./scripts/serve-ios.sh --device "iPhone 16 Pro"
```

#### Specific Platforms

```bash
# Web (default)
dx serve --platform web

# Desktop
dx serve --platform desktop

# Mobile (iOS Simulator)
dx serve --platform ios

# Mobile (Android Emulator)
dx serve --platform android
```

For NixOS convenience (auto create/start emulator + boot wait):

```bash
just serve-android
# alias:
just serve-andoird
```

### Building for Production

#### Desktop Bundles

```bash
dx bundle --platform desktop --release
```

#### Mobile Builds

```bash
# iOS
dx bundle --platform ios --release

# Android
./scripts/bundle-android.sh
# or
just bundle-android-release
```

`scripts/bundle-android.sh` only exports Android release `.apk` artifacts into `dist/android`.

Optional signing env vars for `scripts/bundle-android.sh`:

- `ANDROID_KEYSTORE_BASE64` (or `ANDROID_KEYSTORE_PATH`)
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD` (optional)

CI/CD builds Android release APKs and publishes only `.apk` artifacts from `dist/android`.

#### Apple Bundles (.app + unsigned .ipa)

```bash
./scripts/bundle-apple.sh
```

- macOS `.app` output: `dist/apple/macos`
- iOS `.app` output: `dist/apple/ios`
- Unsigned iOS `.ipa`: `dist/apple/ios/*-unsigned.ipa`

By default, the script builds for physical iOS devices (`aarch64-apple-ios`). To build for the simulator instead:

```bash
IOS_TARGET=aarch64-apple-ios-sim ./scripts/bundle-apple.sh
```

If your shell exports Homebrew C/C++ flags (for example `LDFLAGS`/`LIBRARY_PATH` for `libiconv`), prefer this script over raw `dx bundle --ios` so those vars are unset for iOS linking.

You can also override icon source/name if needed:

```bash
APP_NAME="RustySound" IOS_ICON_SOURCE="/absolute/path/to/icon-1024.png" ./scripts/bundle-apple.sh
```

## Project Structure

```
rustysound/
├── assets/                 # Static assets (icons, styles, etc.)
├── src/
│   ├── main.rs            # Application entry point
│   ├── components/        # Reusable UI components
│   │   ├── app.rs         # Main app component
│   │   ├── player.rs      # Audio player controls
│   │   ├── sidebar.rs     # Navigation sidebar
│   │   └── views/         # Page components
│   │       ├── home.rs    # Home/dashboard
│   │       ├── albums.rs  # Album browser
│   │       ├── artists.rs # Artist browser
│   │       ├── queue.rs   # Playback queue
│   │       └── settings.rs # App settings
│   ├── api/               # Server API integration
│   ├── db/                # Local database/storage
│   └── components.rs      # Component exports
├── Cargo.toml             # Rust dependencies
├── Dioxus.toml           # Dioxus configuration
└── tailwind.css          # Tailwind CSS styles
```

## Configuration

### Server Connection

1. Launch RustySound
2. Go to Settings
3. Add your Navidrome/Subsonic server details:
   - Server URL
   - Username
   - Password (optional, depending on server setup)

### Supported Servers

- **Navidrome**: Full feature support
- **Subsonic**: Compatible with Subsonic API v1.16.1+
- **Airsonic**: Compatible servers
- **Gonic**: Compatible servers

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature`
3. Make your changes and test thoroughly
4. Submit a pull request

### Development Guidelines

- Follow Rust best practices
- Use Dioxus component patterns
- Test on multiple platforms when possible
- Update documentation for new features

## License

Copyright © 2026 AD-Archer. All rights reserved.

## Acknowledgments

- Built with [Dioxus](https://dioxuslabs.com/) - A Rust UI framework
- Audio playback powered by Web Audio API and native platform APIs
- Icons and UI design inspired by modern music streaming applications
