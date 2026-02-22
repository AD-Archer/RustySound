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

# Create an Android emulator (AVD) with sane defaults.
android-avd-create name="rustysound-api34" image="system-images;android-34;google_apis;x86_64" device="pixel_6":
    if [ -f /etc/NIXOS ]; then \
        nix develop -c avdmanager create avd -n "{{name}}" -k "{{image}}" --device "{{device}}"; \
    else \
        avdmanager create avd -n "{{name}}" -k "{{image}}" --device "{{device}}"; \
    fi

# List available Android emulators (AVDs).
android-avd-list:
    if [ -f /etc/NIXOS ]; then \
        nix develop -c emulator -list-avds; \
    else \
        emulator -list-avds; \
    fi

# Start an Android emulator by AVD name.
android-avd-start name="rustysound-api34":
    if [ -f /etc/NIXOS ]; then \
        nix develop -c emulator -avd "{{name}}"; \
    else \
        emulator -avd "{{name}}"; \
    fi

# Serve Android target end-to-end: ensure emulator exists/runs, then start dev server.
serve-android name="rustysound-api34" image="system-images;android-34;google_apis;x86_64" device="pixel_6" *args:
    if [ -f /etc/NIXOS ]; then \
        nix develop -c bash -lc '\
            set -euo pipefail; \
            if ! emulator -list-avds | grep -qx "{{name}}"; then \
                echo "Creating AVD {{name}}..."; \
                echo "no" | avdmanager create avd -n "{{name}}" -k "{{image}}" --device "{{device}}"; \
            fi; \
            if ! adb devices | awk '\''$1 ~ /^emulator-[0-9]+$/ && $2 == "device" { found=1 } END { exit found ? 0 : 1 }'\''; then \
                echo "Starting emulator {{name}}..."; \
                nohup emulator -avd "{{name}}" -no-snapshot-load -netdelay none -netspeed full >/tmp/rustysound-emulator.log 2>&1 & \
                sleep 2; \
                if ! test -f /tmp/rustysound-emulator.log; then \
                    echo "Emulator launch log was not created. Launch likely failed immediately."; \
                    exit 1; \
                fi; \
            fi; \
            echo "Waiting for emulator to be ready..."; \
            adb start-server >/dev/null; \
            dev_ok=0; \
            for _ in $(seq 1 120); do \
                if adb devices | awk '\''$1 ~ /^emulator-[0-9]+$/ && $2 == "device" { found=1 } END { exit found ? 0 : 1 }'\''; then \
                    dev_ok=1; \
                    break; \
                fi; \
                sleep 1; \
            done; \
            if [ "$dev_ok" -ne 1 ]; then \
                echo "ADB did not detect an emulator within 2 minutes."; \
                echo "Last emulator log lines:"; \
                tail -n 80 /tmp/rustysound-emulator.log || true; \
                exit 1; \
            fi; \
            boot_ok=0; \
            for _ in $(seq 1 180); do \
                if [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d "\\r")" = "1" ]; then \
                    boot_ok=1; \
                    break; \
                fi; \
                sleep 2; \
            done; \
            if [ "$boot_ok" -ne 1 ]; then \
                echo "Emulator did not finish booting within 6 minutes."; \
                echo "Check logs: /tmp/rustysound-emulator.log"; \
                tail -n 80 /tmp/rustysound-emulator.log || true; \
                exit 1; \
            fi; \
            run_dx() { \
                local attempt=1; \
                while [ "$attempt" -le 2 ]; do \
                    if RUST_BACKTRACE=1 dx serve --android {{args}}; then \
                        return 0; \
                    fi; \
                    rc=$?; \
                    echo "dx serve exited with code $rc."; \
                    if [ "$attempt" -eq 2 ]; then \
                        return "$rc"; \
                    fi; \
                    echo "Retrying dx serve once (Android dev-server stability workaround)..."; \
                    sleep 2; \
                    attempt=$((attempt + 1)); \
                done; \
            }; \
            run_dx'; \
    else \
        bash -lc '\
            set -euo pipefail; \
            if ! emulator -list-avds | grep -qx "{{name}}"; then \
                echo "Creating AVD {{name}}..."; \
                echo "no" | avdmanager create avd -n "{{name}}" -k "{{image}}" --device "{{device}}"; \
            fi; \
            if ! adb devices | awk '\''$1 ~ /^emulator-[0-9]+$/ && $2 == "device" { found=1 } END { exit found ? 0 : 1 }'\''; then \
                echo "Starting emulator {{name}}..."; \
                nohup emulator -avd "{{name}}" -no-snapshot-load -netdelay none -netspeed full >/tmp/rustysound-emulator.log 2>&1 & \
                sleep 2; \
                if ! test -f /tmp/rustysound-emulator.log; then \
                    echo "Emulator launch log was not created. Launch likely failed immediately."; \
                    exit 1; \
                fi; \
            fi; \
            echo "Waiting for emulator to be ready..."; \
            adb start-server >/dev/null; \
            dev_ok=0; \
            for _ in $(seq 1 120); do \
                if adb devices | awk '\''$1 ~ /^emulator-[0-9]+$/ && $2 == "device" { found=1 } END { exit found ? 0 : 1 }'\''; then \
                    dev_ok=1; \
                    break; \
                fi; \
                sleep 1; \
            done; \
            if [ "$dev_ok" -ne 1 ]; then \
                echo "ADB did not detect an emulator within 2 minutes."; \
                echo "Last emulator log lines:"; \
                tail -n 80 /tmp/rustysound-emulator.log || true; \
                exit 1; \
            fi; \
            boot_ok=0; \
            for _ in $(seq 1 180); do \
                if [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d "\\r")" = "1" ]; then \
                    boot_ok=1; \
                    break; \
                fi; \
                sleep 2; \
            done; \
            if [ "$boot_ok" -ne 1 ]; then \
                echo "Emulator did not finish booting within 6 minutes."; \
                echo "Check logs: /tmp/rustysound-emulator.log"; \
                tail -n 80 /tmp/rustysound-emulator.log || true; \
                exit 1; \
            fi; \
            run_dx() { \
                local attempt=1; \
                while [ "$attempt" -le 2 ]; do \
                    if RUST_BACKTRACE=1 dx serve --android {{args}}; then \
                        return 0; \
                    fi; \
                    rc=$?; \
                    echo "dx serve exited with code $rc."; \
                    if [ "$attempt" -eq 2 ]; then \
                        return "$rc"; \
                    fi; \
                    echo "Retrying dx serve once (Android dev-server stability workaround)..."; \
                    sleep 2; \
                    attempt=$((attempt + 1)); \
                done; \
            }; \
            run_dx'; \
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

# Android mobile compile check (no run).
check-android:
    if [ -f /etc/NIXOS ]; then \
        nix develop -c cargo check --target x86_64-linux-android --features mobile; \
    else \
        cargo check --target x86_64-linux-android --features mobile; \
    fi

# Web compile check.
check-web:
    if [ -f /etc/NIXOS ]; then \
        nix develop -c cargo check --target wasm32-unknown-unknown --features web; \
    else \
        cargo check --target wasm32-unknown-unknown --features web; \
    fi

# Linux desktop compile check.
check-linux:
    if [ -f /etc/NIXOS ]; then \
        nix develop -c cargo check --features desktop; \
    else \
        cargo check --features desktop; \
    fi

# Cross-platform sanity matrix.
check-matrix:
    just check-linux
    just check-web
    just check-android

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
