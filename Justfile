default:
    @just --list

# General development server (web by default).
serve *args:
    dx serve {{args}}

# Default bundle target: Windows installers into dist/windows.
bundle *args:
    dx bundle {{args}}

# Build Windows NSIS installer (.exe).
bundle-windows *args:
    dx bundle --windows --release --package-types nsis --out-dir dist/windows {{args}}
# iOS simulator development server with sanitized linker environment.
serve-ios *args:
    ./scripts/serve-ios.sh {{args}}

# iOS serve variant with an optional URL env override.
# Note: app support for this env var is optional and app-specific.
serve-ios-url url *args:
    RUSTYSOUND_SERVER_URL="{{url}}" ./scripts/serve-ios.sh {{args}}

# Build macOS app + macOS dmg + iOS app + unsigned IPA.
bundle-apple:
    ./scripts/bundle-apple.sh

# Build for iOS simulator instead of physical device.
bundle-sim:
    IOS_TARGET=aarch64-apple-ios-sim ./scripts/bundle-apple.sh

# Quick compile check using default features.
check:
    cargo check

# Apple-focused compile checks.
check-apple:
    cargo check --features desktop
    cargo check --target aarch64-apple-ios --features mobile
    cargo check --target aarch64-apple-ios-sim --features mobile
