#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if ! command -v dx >/dev/null 2>&1; then
    echo "dx CLI is required (https://dioxuslabs.com/learn/0.7/getting_started)." >&2
    exit 1
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
