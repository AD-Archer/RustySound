#!/usr/bin/env bash
# Writes the App Group entitlements for simulator builds, where they live in
# a __TEXT,__entitlements section of the binary (the simulator ignores the
# signature's): build.sh gives it to xcodebuild for the widget, and
# scripts/bundle-apple.sh links it into the app.
#
#   apple/widgets/sim-entitlements.sh <out.plist>
set -euo pipefail
OUT="${1:?usage: sim-entitlements.sh <out.plist>}"
mkdir -p "$(dirname "${OUT}")"
rm -f "${OUT}"
plutil -create xml1 "${OUT}"
/usr/libexec/PlistBuddy -c 'Add :com.apple.security.application-groups array' "${OUT}"
/usr/libexec/PlistBuddy -c "Add :com.apple.security.application-groups:0 string ${RS_APP_GROUP:-group.app.adarcher.rustysound}" "${OUT}"
