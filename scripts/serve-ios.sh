#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"
source "${ROOT_DIR}/scripts/setup-rustup-env.sh"

if ! command -v dx >/dev/null 2>&1; then
    echo "dx CLI is required (https://dioxuslabs.com/learn/0.7/getting_started)." >&2
    exit 1
fi

if ! command -v xcodebuild >/dev/null 2>&1; then
    cat >&2 <<'EOF'
iOS simulator builds require full Xcode (not only Command Line Tools).
Install Xcode from the App Store, open it once, then run:
  sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
  sudo xcodebuild -license accept
  sudo xcodebuild -runFirstLaunch
EOF
    exit 1
fi

if ! xcrun --sdk iphonesimulator --show-sdk-path >/dev/null 2>&1; then
    cat >&2 <<'EOF'
Could not locate the iOS Simulator SDK (iphonesimulator).
If Xcode is installed, select it and finish setup:
  sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
  sudo xcodebuild -license accept
  sudo xcodebuild -runFirstLaunch

If /Applications/Xcode.app does not exist yet, install Xcode first.
EOF
    exit 1
fi

if ! xcrun simctl list devices booted 2>/dev/null | grep -q "Booted"; then
    echo "No booted iOS simulator found. Booting an available simulator..." >&2

    boot_udid="$(
        xcrun simctl list devices available 2>/dev/null \
            | awk -F '[()]' '/iPhone .* \(Shutdown\)$/ {print $2; exit}'
    )"
    if [[ -z "${boot_udid}" ]]; then
        boot_udid="$(
            xcrun simctl list devices available 2>/dev/null \
                | awk -F '[()]' '/\(Shutdown\)$/ {print $2; exit}'
        )"
    fi

    if [[ -z "${boot_udid}" ]]; then
        cat >&2 <<'EOF'
No available iOS simulator devices were found.
Install an iOS simulator runtime in Xcode -> Settings -> Platforms, then open Simulator once.
EOF
        exit 1
    fi

    xcrun simctl boot "${boot_udid}" >/dev/null 2>&1 || true
    open -a Simulator >/dev/null 2>&1 || true
    xcrun simctl bootstatus "${boot_udid}" -b >/dev/null 2>&1 || true
fi

# Keep iOS simulator linking isolated from host/Homebrew toolchain flags.
exec env \
    -u CPATH \
    -u C_INCLUDE_PATH \
    -u CPLUS_INCLUDE_PATH \
    -u CPPFLAGS \
    -u CFLAGS \
    -u LDFLAGS \
    -u LIBRARY_PATH \
    -u PKG_CONFIG_PATH \
    -u PKG_CONFIG_LIBDIR \
    -u PKG_CONFIG_SYSROOT_DIR \
    -u CC \
    -u CXX \
    -u NIX_LDFLAGS \
    -u NIX_CFLAGS_COMPILE \
    dx serve --ios "$@"
