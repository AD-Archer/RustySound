#!/usr/bin/env bash
# Builds the Now Playing widget extension (RustySoundWidgets.appex) and the
# bridge framework the app loads (RustySoundWidgetBridge.framework), unsigned;
# embed.sh signs them inside the app. See docs/WIDGETS.md.
#
#   apple/widgets/build.sh <ios|ios-sim|macos> [out-dir]
#
# Env: RS_APP_BUNDLE_ID (app.adarcher.rustysound; the widget is
# <id>.widgets), RS_APP_GROUP (group.app.adarcher.rustysound).
set -euo pipefail

WIDGETS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${WIDGETS_DIR}/../.." && pwd)"
PLATFORM="${1:?usage: build.sh <ios|ios-sim|macos> [out-dir]}"
OUT_DIR="${2:-${ROOT_DIR}/target/apple-widgets/${PLATFORM}}"
RS_APP_BUNDLE_ID="${RS_APP_BUNDLE_ID:-app.adarcher.rustysound}"
RS_APP_GROUP="${RS_APP_GROUP:-group.app.adarcher.rustysound}"

case "${PLATFORM}" in
    ios) SDK=iphoneos; PRODUCTS="Release-iphoneos" ;;
    ios-sim) SDK=iphonesimulator; PRODUCTS="Release-iphonesimulator" ;;
    macos) SDK=macosx; PRODUCTS="Release" ;;
    *) echo "Unknown widget platform: ${PLATFORM} (ios, ios-sim or macos)" >&2; exit 1 ;;
esac

# xcodebuild ships with Xcode, not the Command Line Tools.
if [[ -z "${DEVELOPER_DIR:-}" && -d /Applications/Xcode.app/Contents/Developer ]] \
    && ! xcrun --sdk "${SDK}" --show-sdk-path >/dev/null 2>&1; then
    export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
fi

mkdir -p "${OUT_DIR}"
SIGNING=()
if [[ "${PLATFORM}" == "ios-sim" ]]; then
    # The simulator reads entitlements from a section in the binary, which
    # Xcode writes only when it signs; ad-hoc is enough.
    "${WIDGETS_DIR}/sim-entitlements.sh" "${OUT_DIR}/simulated.entitlements"
    SIGNING=(CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=- CODE_SIGN_ENTITLEMENTS="${OUT_DIR}/simulated.entitlements")
fi
echo "Building the Now Playing widget for ${PLATFORM}..."
xcodebuild \
    -project "${WIDGETS_DIR}/RustySoundWidgets.xcodeproj" \
    -target RustySoundWidgets \
    -target RustySoundWidgetBridge \
    -configuration Release \
    -sdk "${SDK}" \
    -quiet \
    ARCHS=arm64 \
    ONLY_ACTIVE_ARCH=NO \
    SYMROOT="${OUT_DIR}/build" \
    OBJROOT="${OUT_DIR}/obj" \
    RS_APP_BUNDLE_ID="${RS_APP_BUNDLE_ID}" \
    RS_APP_GROUP="${RS_APP_GROUP}" \
    ${SIGNING[@]+"${SIGNING[@]}"} \
    build

for product in RustySoundWidgets.appex RustySoundWidgetBridge.framework; do
    rm -rf "${OUT_DIR:?}/${product}"
    cp -R "${OUT_DIR}/build/${PRODUCTS}/${product}" "${OUT_DIR}/${product}"
done
echo "Widget products: ${OUT_DIR}"
