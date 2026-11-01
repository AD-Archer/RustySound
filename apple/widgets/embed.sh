#!/usr/bin/env bash
# Embeds the Now Playing widget (from build.sh) into a bundled RustySound.app
# and signs the result inside out: the bridge framework, the widget, the app.
# See docs/WIDGETS.md.
#
#   apple/widgets/embed.sh <ios|ios-sim|macos> <path/to/App.app> [widgets-dir]
#
# Env (<P> is IOS or MACOS, so one bundle-apple.sh run can sign both):
#   RS_APP_GROUP              App Group id (group.app.adarcher.rustysound).
#   RS_<P>_CODESIGN_IDENTITY  Signing identity. macOS defaults to "-"
#                             (ad-hoc). iOS unset leaves the app unsigned (the
#                             unsigned .ipa path; a re-signing tool signs it
#                             later). The simulator is always ad-hoc.
#   RS_<P>_APP_PROFILE        Provisioning profile for the app; its
#                             entitlements are used for the app.
#   RS_<P>_WIDGETS_PROFILE    Provisioning profile for the widget extension.
set -euo pipefail

WIDGETS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${WIDGETS_DIR}/../.." && pwd)"
PLATFORM="${1:?usage: embed.sh <ios|ios-sim|macos> <App.app> [widgets-dir]}"
APP_PATH="${2:?usage: embed.sh <ios|ios-sim|macos> <App.app> [widgets-dir]}"
BUILT_DIR="${3:-${ROOT_DIR}/target/apple-widgets/${PLATFORM}}"
RS_APP_GROUP="${RS_APP_GROUP:-group.app.adarcher.rustysound}"

case "${PLATFORM}" in
    ios | ios-sim)
        CONTENTS="${APP_PATH}"
        PROFILE_NAME="embedded.mobileprovision"
        ;;
    macos)
        CONTENTS="${APP_PATH}/Contents"
        PROFILE_NAME="embedded.provisionprofile"
        ;;
    *) echo "Unknown widget platform: ${PLATFORM}" >&2; exit 1 ;;
esac
case "${PLATFORM}" in
    ios)
        IDENTITY="${RS_IOS_CODESIGN_IDENTITY:-}"
        APP_PROFILE="${RS_IOS_APP_PROFILE:-}"
        WIDGETS_PROFILE="${RS_IOS_WIDGETS_PROFILE:-}"
        ;;
    ios-sim)
        IDENTITY="-"
        APP_PROFILE=""
        WIDGETS_PROFILE=""
        ;;
    macos)
        IDENTITY="${RS_MACOS_CODESIGN_IDENTITY:--}"
        APP_PROFILE="${RS_MACOS_APP_PROFILE:-}"
        WIDGETS_PROFILE="${RS_MACOS_WIDGETS_PROFILE:-}"
        ;;
esac

APPEX="${CONTENTS}/PlugIns/RustySoundWidgets.appex"
BRIDGE="${CONTENTS}/Frameworks/RustySoundWidgetBridge.framework"
APP_PLIST="${CONTENTS}/Info.plist"
if [[ "${PLATFORM}" == "macos" ]]; then
    APPEX_PLIST="${APPEX}/Contents/Info.plist"
    BRIDGE_PLIST="${BRIDGE}/Versions/A/Resources/Info.plist"
else
    APPEX_PLIST="${APPEX}/Info.plist"
    BRIDGE_PLIST="${BRIDGE}/Info.plist"
fi

for product in RustySoundWidgets.appex RustySoundWidgetBridge.framework; do
    if [[ ! -d "${BUILT_DIR}/${product}" ]]; then
        echo "Missing ${BUILT_DIR}/${product}; run apple/widgets/build.sh ${PLATFORM} first." >&2
        exit 1
    fi
done

echo "Embedding the Now Playing widget in ${APP_PATH}..."
mkdir -p "${CONTENTS}/PlugIns" "${CONTENTS}/Frameworks"
rm -rf "${APPEX}" "${BRIDGE}"
cp -R "${BUILT_DIR}/RustySoundWidgets.appex" "${APPEX}"
cp -R "${BUILT_DIR}/RustySoundWidgetBridge.framework" "${BRIDGE}"
# The app loads the bridge with dlopen; it needs no headers or module.
rm -rf "${BRIDGE}/Headers" "${BRIDGE}/Modules" "${BRIDGE}/Versions/A/Headers" "${BRIDGE}/Versions/A/Modules"

set_plist_string() {
    /usr/libexec/PlistBuddy -c "Set :$2 $3" "$1" >/dev/null 2>&1 \
        || /usr/libexec/PlistBuddy -c "Add :$2 string $3" "$1"
}

# Both sides find the container through RSAppGroup; the widget's version must
# match the app's.
SHORT_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "${APP_PLIST}" 2>/dev/null || echo 1.0)"
BUILD_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "${APP_PLIST}" 2>/dev/null || echo 1)"
set_plist_string "${APP_PLIST}" RSAppGroup "${RS_APP_GROUP}"
for plist in "${APPEX_PLIST}" "${BRIDGE_PLIST}"; do
    set_plist_string "${plist}" CFBundleShortVersionString "${SHORT_VERSION}"
    set_plist_string "${plist}" CFBundleVersion "${BUILD_VERSION}"
done
set_plist_string "${APPEX_PLIST}" RSAppGroup "${RS_APP_GROUP}"

if [[ -z "${IDENTITY}" ]]; then
    echo "No RS_IOS_CODESIGN_IDENTITY: leaving the app and widget unsigned."
    echo "The re-signing tool must sign the widget with the App Group ${RS_APP_GROUP}."
    exit 0
fi

ENTITLEMENTS_DIR="$(mktemp -d)"
trap 'rm -rf "${ENTITLEMENTS_DIR}"' EXIT

# Entitlements for one bundle: the profile's (device / Developer ID builds),
# or the app's current ones, or none; then the App Group (and, for the Mac
# widget, the sandbox every extension needs).
make_entitlements() {
    local out="$1" profile="$2" existing="$3" sandbox="$4"
    if [[ -n "${profile}" ]]; then
        security cms -D -i "${profile}" > "${ENTITLEMENTS_DIR}/profile.plist"
        /usr/libexec/PlistBuddy -x -c 'Print :Entitlements' "${ENTITLEMENTS_DIR}/profile.plist" > "${out}"
    elif [[ -n "${existing}" ]] && codesign -d --entitlements - --xml "${existing}" > "${out}" 2>/dev/null && [[ -s "${out}" ]]; then
        :
    else
        plutil -create xml1 "${out}"
    fi
    /usr/libexec/PlistBuddy -c 'Delete :com.apple.security.application-groups' "${out}" >/dev/null 2>&1 || true
    /usr/libexec/PlistBuddy -c 'Add :com.apple.security.application-groups array' "${out}"
    /usr/libexec/PlistBuddy -c "Add :com.apple.security.application-groups:0 string ${RS_APP_GROUP}" "${out}"
    if [[ "${sandbox}" == "1" ]]; then
        /usr/libexec/PlistBuddy -c 'Delete :com.apple.security.app-sandbox' "${out}" >/dev/null 2>&1 || true
        /usr/libexec/PlistBuddy -c 'Add :com.apple.security.app-sandbox bool true' "${out}"
    fi
}

SANDBOX_WIDGET=0
[[ "${PLATFORM}" == "macos" ]] && SANDBOX_WIDGET=1
make_entitlements "${ENTITLEMENTS_DIR}/app.plist" "${APP_PROFILE}" "${APP_PATH}" 0
make_entitlements "${ENTITLEMENTS_DIR}/widget.plist" "${WIDGETS_PROFILE}" "" "${SANDBOX_WIDGET}"

if [[ -n "${WIDGETS_PROFILE}" ]]; then
    if [[ "${PLATFORM}" == "macos" ]]; then
        cp "${WIDGETS_PROFILE}" "${APPEX}/Contents/${PROFILE_NAME}"
    else
        cp "${WIDGETS_PROFILE}" "${APPEX}/${PROFILE_NAME}"
    fi
fi
if [[ -n "${APP_PROFILE}" ]]; then
    cp "${APP_PROFILE}" "${CONTENTS}/${PROFILE_NAME}"
fi

SIGN_FLAGS=(--force --sign "${IDENTITY}")
if [[ "${PLATFORM}" == "macos" && "${IDENTITY}" != "-" ]]; then
    # Developer ID: hardened runtime and a timestamp, for notarization.
    SIGN_FLAGS+=(--options runtime --timestamp)
fi

echo "Signing with identity '${IDENTITY}'..."
codesign "${SIGN_FLAGS[@]}" "${BRIDGE}"
codesign "${SIGN_FLAGS[@]}" --entitlements "${ENTITLEMENTS_DIR}/widget.plist" "${APPEX}"
codesign "${SIGN_FLAGS[@]}" --entitlements "${ENTITLEMENTS_DIR}/app.plist" "${APP_PATH}"
codesign --verify --strict "${APP_PATH}"
echo "Widget embedded and signed."
