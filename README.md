# RustySound

A lightweight cross-platform music streaming client for Navidrome and Subsonic-compatible servers, built with Rust and Dioxus, < 15mb

<div style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; margin: 20px 0;">
  <img src="https://www.antonioarcher.com/images/projects/rustysound/desktop/sound_menu.webp" alt="RustySound desktop screenshot" style="width: 100%; border-radius: 6px;" />
  <img src="https://www.antonioarcher.com/images/projects/rustysound/desktop/shot.gif" alt="RustySound lyrics demo" style="width: 100%; border-radius: 6px;" />
  <img src="https://www.antonioarcher.com/images/projects/rustysound/desktop/desktoptheme1.webp" alt="RustySound desktop theme" style="width: 100%; border-radius: 6px;" />
  <img src="https://www.antonioarcher.com/images/projects/rustysound/desktop/desktoptheme2.webp" alt="RustySound desktop theme variant" style="width: 100%; border-radius: 6px;" />
  <img src="https://www.antonioarcher.com/images/projects/rustysound/mobile/mobiletheme1.webp" alt="RustySound mobile theme" style="width: 100%; border-radius: 6px;" />
    <img src="https://github.com/user-attachments/assets/bc93c22e-bcf1-4a41-9d3c-fdba82a36214" alt="RustySound album view" style="width: 100%; border-radius: 6px;" />

</div>

## Features

- **Multi-platform**: Desktop (macOS, Windows, Linux), Mobile (iOS, Android), and Web
- **Audio streaming**: High-quality playback with queue management
- **Server integration**: Connect to Navidrome and Subsonic-compatible servers
- **Offline support**: Local storage, downloads, persistent settings and playback state
- **Playlist management**: Create and organize playlists
- **Browse and search**: Find music by artist, album, or track
- **Full controls**: Play, pause, skip, shuffle, repeat, and more
- **Modern UI**: Clean, responsive Tailwind CSS interface
- **Customizable themes**: Multiple built-in themes with CSS overrides

## Supported Platforms

| Platform    | Installation                                               |
| ----------- | ---------------------------------------------------------- |
| **iOS**     | AltStore (recommended), manual sideloading, or source feed |
| **Android** | APK from Releases                                          |
| **macOS**   | Homebrew, DMG installer                                    |
| **Windows** | Scoop, standalone EXE                                      |
| **Linux**   | Flatpak                                                    |
| **Web**     | Browser + PWA, Docker                                      |

> **Note**: Android is feature-aligned but not under active day-to-day development and may have platform-specific issues.

## Installation

### iOS

#### AltStore Installation (Recommended)

This is the easiest way to install and keep RustySound updated on iOS.

<a href="https://stikstore.app/altdirect/?url=https://ad-archer.github.io/packages/source.json" target="_blank">
<img src="https://github.com/CelloSerenity/altdirect/blob/main/assets/png/AltSource_Blue.png?raw=true" alt="Add to AltStore" width="200">
</a>

Or manually add `https://ad-archer.github.io/packages/source.json` as a source in:

- [AltStore](https://altstore.io/) or [AltServer](https://altstore.io/) for Mac/Windows
- [LiveContainer](https://github.com/LiveContainer/LiveContainer) on iOS

#### Manual Sideloading

1. Download the latest `.ipa` file from [Releases](https://github.com/AD-Archer/RustySound/releases)
2. Use [AltStore](https://altstore.io/), [Sideloadly](https://sideloadly.io/), or [Xcode](https://developer.apple.com/xcode/) to install

### Android

1. Download the latest `.apk` file from [Releases](https://github.com/AD-Archer/RustySound/releases)
2. Enable "Installation from unknown sources" in Settings
3. Install the APK and launch RustySound

### Desktop

#### macOS

##### Homebrew (Recommended)

```bash
brew tap ad-archer/homebrew-tap
brew install --cask rustysound
```

To update:

```bash
brew upgrade rustysound
```

##### DMG Installer

1. Download the latest `.dmg` file from [Releases](https://github.com/AD-Archer/RustySound/releases)
2. Open the DMG and drag RustySound to your Applications folder

**Troubleshooting**: If macOS says the app is damaged or won't open:

```bash
# Option 1: Right-click in Finder and choose Open, then confirm
# Option 2: Run this command
xattr -dr com.apple.quarantine /Applications/RustySound.app
open /Applications/RustySound.app
```

#### Windows

##### Scoop (Recommended)

```powershell
scoop bucket add ad-archer https://github.com/ad-archer/scoop
scoop install ad-archer/rustysound
```

##### Portable Executable

1. Download the latest `.exe` file from [Releases](https://github.com/AD-Archer/RustySound/releases)
2. Open the exe and install RustySound

> **Note**: Antivirus may flag the installer since it is not verified by Windows. This is safe to ignore for builds from the official repository.

#### Linux

##### Flatpak (Recommended)

```bash
flatpak remote-add --if-not-exists --user adarcher-rustysound https://ad-archer.github.io/packages/rustysound.flatpakrepo
flatpak install --user adarcher-rustysound app.adarcher.rustysound//stable
flatpak run app.adarcher.rustysound
```

To update:

```bash
flatpak install --user adarcher-rustysound app.adarcher.rustysound//stable
```

To remove:

```bash
flatpak uninstall --user app.adarcher.rustysound
flatpak remote-delete --user adarcher-rustysound
```

**Troubleshooting — missing GNOME runtime**

If you see an error like:

```
error: The application app.adarcher.rustysound/x86_64/master requires the runtime org.gnome.Platform/x86_64/49 which was not found
```

Install the runtime from Flathub:

```bash
# Add Flathub (if not already present)
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo

# Install the GNOME 49 runtime
flatpak install --user flathub org.gnome.Platform//49

# Reinstall the app
flatpak install --user adarcher-rustysound app.adarcher.rustysound//stable
```

For system-wide install, omit `--user` from the commands.

### Web

Visit [rustysound](https://rustysound-demo.adarcher.app) to try the web version.

#### Docker Deployment

##### Docker Compose (Recommended)

1. Ensure you have Docker and Docker Compose installed
2. Clone this repository or copy [`docker-compose.yml`](https://raw.githubusercontent.com/AD-Archer/RustySound/refs/heads/main/docker-compose.yml)
3. Run:

```bash
docker-compose up -d
```

The web interface will be available at `http://localhost:8080`.

To stop:

```bash
docker-compose down
```

##### Manual Docker Run

```bash
docker run -d -p 8080:80 --name rustysound ghcr.io/ad-archer/rustysound:latest
```

## Development

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

### Running Locally

#### Web

```bash
dx serve --platform web
```

#### Desktop

```bash
dx serve --platform desktop
```

#### Mobile

##### iOS Simulator

```bash
./scripts/serve-ios.sh
```

You can pass `dx serve` options:

```bash
./scripts/serve-ios.sh --device "iPhone 16 Pro"
```

##### Android Emulator

```bash
dx serve --platform android
# or with auto-setup (NixOS):
just serve-android
```

### Quick Commands

Use `just` for common tasks:

```bash
just              # list all recipes
just serve        # dx serve (web)
just serve-ios    # iOS simulator dev (safe linker env)
just serve-android # Android dev/debug (auto-create/start emulator)
just check        # cargo check
just bundle       # macOS + iOS + unsigned IPA
just bundle-android-release # Android release APK
```

### Building for Release

#### Desktop

```bash
dx bundle --platform desktop --release
```

#### iOS

```bash
dx bundle --platform ios --release
```

Or use the Apple bundling script:

```bash
./scripts/bundle-apple.sh
```

Outputs:

- macOS `.app`: `dist/apple/macos`
- iOS `.app`: `dist/apple/ios`
- Unsigned iOS `.ipa`: `dist/apple/ios/*-unsigned.ipa`

To build for simulator:

```bash
IOS_TARGET=aarch64-apple-ios-sim ./scripts/bundle-apple.sh
```

If your shell exports compiler flags (e.g., `LDFLAGS`), use the script to avoid linking issues.

Customize the icon and app name:

```bash
APP_NAME="RustySound" IOS_ICON_SOURCE="/absolute/path/to/icon-1024.png" ./scripts/bundle-apple.sh
```

#### Android

```bash
./scripts/bundle-android.sh
# or
just bundle-android-release
```

This exports release `.apk` to `dist/android`.

Optional signing environment variables:

- `ANDROID_KEYSTORE_BASE64` or `ANDROID_KEYSTORE_PATH`
- `ANDROID_KEYSTORE_PASSWORD`
- `ANDROID_KEY_ALIAS`
- `ANDROID_KEY_PASSWORD` (optional)

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

Licensed under the GNU General Public License v3.0 (GPL-3.0-or-later).  
See [LICENSE](./LICENSE) for full terms.

## Acknowledgments

- Built with [Dioxus](https://dioxuslabs.com/) - A Rust UI framework
- Audio playback powered by Web Audio API and native platform APIs
- Icons and UI design inspired by modern music streaming applications
