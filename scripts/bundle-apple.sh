#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist/apple"
MACOS_OUT_DIR="${DIST_DIR}/macos"
IOS_OUT_DIR="${DIST_DIR}/ios"
IOS_TARGET="${IOS_TARGET:-aarch64-apple-ios}"
APP_NAME="${APP_NAME:-RustySound}"
IOS_ICON_SOURCE="${IOS_ICON_SOURCE:-${ROOT_DIR}/assets/web-app-manifest-512x512.png}"
DMG_INSTALL_FILE_NAME="${DMG_INSTALL_FILE_NAME:-INSTALL.txt}"

create_macos_dmg_with_instructions() {
    local app_path="$1"
    local dmg_path="$2"
    local app_name="$3"
    local dmg_stage_dir
    dmg_stage_dir="$(mktemp -d)"

    cp -R "${app_path}" "${dmg_stage_dir}/"
    cat > "${dmg_stage_dir}/${DMG_INSTALL_FILE_NAME}" <<EOF
RustySound macOS Install Instructions
=====================================

1. Drag ${app_name}.app to the Applications folder.
2. Open ${app_name}.app from Applications.
3. If macOS blocks it:
   - Open System Settings > Privacy & Security
   - Click "Open Anyway" for ${app_name}.app
   - Or right-click the app and choose Open

If needed, in Terminal:
xattr -dr com.apple.quarantine /Applications/${app_name}.app
open /Applications/${app_name}.app
EOF

    rm -f "${dmg_path}"
    hdiutil create -volname "${app_name}" -srcfolder "${dmg_stage_dir}" -ov -format UDZO "${dmg_path}"
    rm -rf "${dmg_stage_dir}"
}

if ! command -v dx >/dev/null 2>&1; then
    echo "dx CLI is required (https://dioxuslabs.com/learn/0.7/getting_started)." >&2
    exit 1
fi

if ! command -v zip >/dev/null 2>&1; then
    echo "zip is required to create the unsigned .ipa archive." >&2
    exit 1
fi

if ! command -v sips >/dev/null 2>&1; then
    echo "sips is required to generate iOS icon sizes." >&2
    exit 1
fi

if [[ ! -f "${IOS_ICON_SOURCE}" ]]; then
    echo "iOS icon source not found: ${IOS_ICON_SOURCE}" >&2
    exit 1
fi

if [[ "${IOS_TARGET}" == *"-ios-sim" ]]; then
    IOS_SDK="iphonesimulator"
else
    IOS_SDK="iphoneos"
fi

mkdir -p "${MACOS_OUT_DIR}" "${IOS_OUT_DIR}"

echo "Bundling macOS app (.app)..."
dx bundle \
    --macos \
    --package-types macos \
    --features desktop \
    --release \
    --out-dir "${MACOS_OUT_DIR}"

if ! xcrun --sdk "${IOS_SDK}" --show-sdk-path >/dev/null 2>&1; then
    echo "Missing Apple SDK '${IOS_SDK}'. Install/enable Xcode + command line tools, then retry." >&2
    exit 1
fi

echo "Bundling iOS app (.app) without signing..."
env \
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
    dx bundle \
        --ios \
        --package-types ios \
        --features mobile \
        --target "${IOS_TARGET}" \
        --codesign false \
        --release \
        --out-dir "${IOS_OUT_DIR}"

IOS_APP_PATH="$(find "${IOS_OUT_DIR}" -maxdepth 1 -type d -name '*.app' | head -n 1)"
if [[ -z "${IOS_APP_PATH}" ]]; then
    echo "Could not find an iOS .app in ${IOS_OUT_DIR}" >&2
    exit 1
fi

IOS_APP_NAME="$(basename "${IOS_APP_PATH}" .app)"
IPA_PATH="${IOS_OUT_DIR}/${IOS_APP_NAME}-unsigned.ipa"
IOS_PLIST_PATH="${IOS_APP_PATH}/Info.plist"

echo "Generating iOS app icons..."
sips -s format png -z 120 120 "${IOS_ICON_SOURCE}" --out "${IOS_APP_PATH}/AppIcon60x60@2x.png" >/dev/null
sips -s format png -z 180 180 "${IOS_ICON_SOURCE}" --out "${IOS_APP_PATH}/AppIcon60x60@3x.png" >/dev/null
sips -s format png -z 152 152 "${IOS_ICON_SOURCE}" --out "${IOS_APP_PATH}/AppIcon76x76@2x.png" >/dev/null
sips -s format png -z 167 167 "${IOS_ICON_SOURCE}" --out "${IOS_APP_PATH}/AppIcon83.5x83.5@2x.png" >/dev/null

echo "Updating iOS Info.plist metadata..."
/usr/libexec/PlistBuddy -c "Set :CFBundleDisplayName ${APP_NAME}" "${IOS_PLIST_PATH}" \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleDisplayName string ${APP_NAME}" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Set :CFBundleName ${APP_NAME}" "${IOS_PLIST_PATH}" \
    || /usr/libexec/PlistBuddy -c "Add :CFBundleName string ${APP_NAME}" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Delete :UIBackgroundModes" "${IOS_PLIST_PATH}" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Add :UIBackgroundModes array" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :UIBackgroundModes:0 string audio" "${IOS_PLIST_PATH}"

# Allow streaming/audio URLs in WKWebView for sideload/dev builds (especially HTTP radio streams).
/usr/libexec/PlistBuddy -c "Delete :NSAppTransportSecurity" "${IOS_PLIST_PATH}" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Add :NSAppTransportSecurity dict" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :NSAppTransportSecurity:NSAllowsArbitraryLoads bool true" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :NSAppTransportSecurity:NSAllowsArbitraryLoadsInWebContent bool true" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :NSAppTransportSecurity:NSAllowsArbitraryLoadsForMedia bool true" "${IOS_PLIST_PATH}"

# Local-network servers (Navidrome on LAN) may require this usage description on device.
/usr/libexec/PlistBuddy -c "Set :NSLocalNetworkUsageDescription RustySound needs local network access to stream from your media server." "${IOS_PLIST_PATH}" \
    || /usr/libexec/PlistBuddy -c "Add :NSLocalNetworkUsageDescription string RustySound needs local network access to stream from your media server." "${IOS_PLIST_PATH}"

/usr/libexec/PlistBuddy -c "Delete :CFBundleIcons" "${IOS_PLIST_PATH}" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Delete :CFBundleIcons~ipad" "${IOS_PLIST_PATH}" >/dev/null 2>&1 || true

/usr/libexec/PlistBuddy -c "Add :CFBundleIcons dict" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons:CFBundlePrimaryIcon dict" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons:CFBundlePrimaryIcon:CFBundleIconFiles array" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons:CFBundlePrimaryIcon:CFBundleIconFiles:0 string AppIcon60x60" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons:CFBundlePrimaryIcon:UIPrerenderedIcon bool false" "${IOS_PLIST_PATH}"

/usr/libexec/PlistBuddy -c "Add :CFBundleIcons~ipad dict" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons~ipad:CFBundlePrimaryIcon dict" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons~ipad:CFBundlePrimaryIcon:CFBundleIconFiles array" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons~ipad:CFBundlePrimaryIcon:CFBundleIconFiles:0 string AppIcon76x76" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons~ipad:CFBundlePrimaryIcon:CFBundleIconFiles:1 string AppIcon83.5x83.5" "${IOS_PLIST_PATH}"
/usr/libexec/PlistBuddy -c "Add :CFBundleIcons~ipad:CFBundlePrimaryIcon:UIPrerenderedIcon bool false" "${IOS_PLIST_PATH}"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

mkdir -p "${TMP_DIR}/Payload"
cp -R "${IOS_APP_PATH}" "${TMP_DIR}/Payload/"

(
    cd "${TMP_DIR}"
    zip -qry "${IPA_PATH}" Payload
)

MACOS_APP_PATH="$(find "${MACOS_OUT_DIR}" -maxdepth 1 -type d -name '*.app' | head -n 1)"

if [[ -z "${MACOS_APP_PATH}" ]]; then
    echo "Could not find a macOS .app in ${MACOS_OUT_DIR}" >&2
    exit 1
fi

MACOS_APP_NAME="$(basename "${MACOS_APP_PATH}" .app)"
MACOS_DMG_PATH="${MACOS_OUT_DIR}/${MACOS_APP_NAME}.dmg"
echo "Creating macOS installer (.dmg) with install instructions..."
create_macos_dmg_with_instructions "${MACOS_APP_PATH}" "${MACOS_DMG_PATH}" "${MACOS_APP_NAME}"

echo "macOS bundle output: ${MACOS_OUT_DIR}"
echo "macOS app: ${MACOS_APP_PATH}"
echo "macOS dmg: ${MACOS_DMG_PATH}"
echo "iOS bundle output: ${IOS_OUT_DIR}"
echo "Unsigned IPA: ${IPA_PATH}"
