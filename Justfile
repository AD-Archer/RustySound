default:
    @just --list

# Enter the Nix development shell defined in flake.nix.
nix:
    nix develop

# Back-compat alias.
nix-dev:
    nix develop

# Bootstrap Rust + Dioxus CLI inside the Nix dev shell.
nix-bootstrap:
    nix develop -c bash -lc 'rustup toolchain install stable && rustup default stable && rustup target add wasm32-unknown-unknown && rm -f "$HOME/.cargo/bin/dx" && cargo install dioxus-cli --locked --force && command -v dx && dx --version'

# Serve app through the Nix dev shell (Linux desktop).
nix-serve:
    nix develop -c dx serve --linux

# Bundle app through the Nix dev shell (Linux desktop).
nix-bundle:
    nix develop -c dx bundle --linux --out-dir dist/linux

# General development server (web by default).
serve *args:
    if [ -f /etc/NIXOS ]; then \
        nix develop -c dx serve {{args}}; \
    else \
        dx serve {{args}}; \
    fi

# Default bundle target: Windows installers into dist/windows.
bundle *args:
    if [ -f /etc/NIXOS ]; then \
        nix develop -c dx bundle {{args}}; \
    else \
        dx bundle {{args}}; \
    fi

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

# Linux desktop dev server (Dioxus desktop platform).
serve-linux:
    if [ -f /etc/NIXOS ]; then \
        just nix-serve; \
    else \
        dx serve --linux; \
    fi

# Linux desktop server in release mode for smoother runtime perf checks.
serve-linux-release:
    if [ -f /etc/NIXOS ]; then \
        nix develop -c dx serve --linux --release; \
    else \
        dx serve --linux --release; \
    fi

# Linux desktop bundle (Dioxus desktop platform).
bundle-linux:
    if [ -f /etc/NIXOS ]; then \
        just nix-bundle; \
    else \
        dx bundle --linux --out-dir dist/linux; \
    fi

# Run a built Linux desktop binary inside the Nix runtime environment.
run-linux-binary path="./dist/linux/rustysound":
    nix develop -c "{{path}}"

merge:
    git push && git checkout prod && git merge main && git push && git checkout main
