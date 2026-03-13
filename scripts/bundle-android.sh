#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="${OUT_DIR:-dist/android}"
mkdir -p "$OUT_DIR"

find_release_apks() {
    find target/dx -path "*/release/android/*" -type f -name "*.apk" -print0
}

find_release_aabs() {
    find target/dx -path "*/release/android/*" -type f -name "*.aab" -print0
}

find_gradle_root() {
    local dir
    dir="$(dirname "$1")"

    while [ "$dir" != "/" ] && [ "$dir" != "." ]; do
        if [ -x "$dir/gradlew" ] || [ -f "$dir/settings.gradle" ] || [ -f "$dir/settings.gradle.kts" ]; then
            printf '%s\n' "$dir"
            return 0
        fi
        dir="$(dirname "$dir")"
    done

    return 1
}

build_release_apks_from_aabs() {
    local gradle_bin=""
    local root=""
    local aab=""
    local -a gradle_roots=()

    mapfile -t gradle_roots < <(
        while IFS= read -r -d '' aab; do
            root="$(find_gradle_root "$aab" || true)"
            if [ -n "$root" ]; then
                printf '%s\n' "$root"
            fi
        done < <(find_release_aabs) | sort -u
    )

    if [ "${#gradle_roots[@]}" -eq 0 ]; then
        return 1
    fi

    echo "Dioxus produced an Android App Bundle (.aab) but no installable APK."
    echo "Trying Gradle assembleRelease to produce a distributable APK..."

    for root in "${gradle_roots[@]}"; do
        if [ -x "$root/gradlew" ]; then
            (
                cd "$root"
                ./gradlew assembleRelease
            )
            continue
        fi

        if [ -z "$gradle_bin" ]; then
            gradle_bin="$(command -v gradle || true)"
        fi

        if [ -z "$gradle_bin" ]; then
            echo "Gradle is required to convert the generated Android project into an APK." >&2
            echo "Install Gradle or ensure the Dioxus Android project includes a gradle wrapper." >&2
            return 1
        fi

        (
            cd "$root"
            "$gradle_bin" assembleRelease
        )
    done
}

echo "Building Android release APK via Dioxus..."
dx bundle --platform android --release "$@"

find "$OUT_DIR" -maxdepth 1 -type f \
    \( -name "*.apk" -o -name "*.aab" -o -name "*.apks" \) \
    -delete

echo "Collecting Android release APK artifacts..."
mapfile -d '' RELEASE_APKS < <(find_release_apks)

if [ "${#RELEASE_APKS[@]}" -eq 0 ]; then
    if ! build_release_apks_from_aabs; then
        echo "Gradle fallback did not produce an Android release APK." >&2
    fi
    mapfile -d '' RELEASE_APKS < <(find_release_apks)
fi

if [ "${#RELEASE_APKS[@]}" -eq 0 ]; then
    echo "No Android release APK artifacts were found under target/dx." >&2
    echo "RustySound only publishes installable APK files for Android releases." >&2
    exit 1
fi

for artifact in "${RELEASE_APKS[@]}"; do
    cp "$artifact" "$OUT_DIR/"
done

SIGNED=0
KEYSTORE_FILE=""
if [ -n "${ANDROID_KEYSTORE_BASE64:-}" ]; then
    KEYSTORE_FILE="$(mktemp "${TMPDIR:-/tmp}/rustysound-android-keystore-XXXXXX.jks")"
    if printf '%s' "$ANDROID_KEYSTORE_BASE64" | base64 --decode > "$KEYSTORE_FILE" 2>/dev/null; then
        :
    else
        printf '%s' "$ANDROID_KEYSTORE_BASE64" | base64 -D > "$KEYSTORE_FILE"
    fi
elif [ -n "${ANDROID_KEYSTORE_PATH:-}" ]; then
    KEYSTORE_FILE="$ANDROID_KEYSTORE_PATH"
fi

if [ -n "$KEYSTORE_FILE" ] \
    && [ -n "${ANDROID_KEY_ALIAS:-}" ] \
    && [ -n "${ANDROID_KEYSTORE_PASSWORD:-}" ]; then
    echo "Signing Android release APK artifacts..."

    APKSIGNER_BIN="$(command -v apksigner || true)"
    ZIPALIGN_BIN="$(command -v zipalign || true)"

    if [ -z "$APKSIGNER_BIN" ] && [ -n "${ANDROID_HOME:-}" ] && [ -d "${ANDROID_HOME}/build-tools" ]; then
        APKSIGNER_BIN="$(find "${ANDROID_HOME}/build-tools" -type f -name apksigner | sort -V | tail -n 1 || true)"
    fi
    if [ -z "$ZIPALIGN_BIN" ] && [ -n "${ANDROID_HOME:-}" ] && [ -d "${ANDROID_HOME}/build-tools" ]; then
        ZIPALIGN_BIN="$(find "${ANDROID_HOME}/build-tools" -type f -name zipalign | sort -V | tail -n 1 || true)"
    fi

    if [ -z "$APKSIGNER_BIN" ]; then
        echo "apksigner not found; cannot sign APKs." >&2
        exit 1
    fi

    shopt -s nullglob
    for apk in "$OUT_DIR"/*.apk; do
        aligned_apk="${apk%.apk}-aligned.apk"
        signed_apk="${apk%.apk}-signed.apk"

        input_apk="$apk"
        if [ -n "$ZIPALIGN_BIN" ]; then
            "$ZIPALIGN_BIN" -f -p 4 "$apk" "$aligned_apk"
            input_apk="$aligned_apk"
        fi

        APKSIGNER_ARGS=(
            "$APKSIGNER_BIN" sign
            --ks "$KEYSTORE_FILE"
            --ks-key-alias "$ANDROID_KEY_ALIAS"
            --ks-pass "pass:${ANDROID_KEYSTORE_PASSWORD}"
            --out "$signed_apk"
            "$input_apk"
        )
        if [ -n "${ANDROID_KEY_PASSWORD:-}" ]; then
            APKSIGNER_ARGS+=(--key-pass "pass:${ANDROID_KEY_PASSWORD}")
        fi
        "${APKSIGNER_ARGS[@]}"
        "$APKSIGNER_BIN" verify "$signed_apk"

        rm -f "$apk" "$aligned_apk"
        SIGNED=1
    done
    shopt -u nullglob
fi

if [ "$SIGNED" -eq 1 ]; then
    echo "Android release APK signing complete."
else
    echo "Android release APKs are unsigned." >&2
    echo "Set ANDROID_KEYSTORE_BASE64 or ANDROID_KEYSTORE_PATH, ANDROID_KEYSTORE_PASSWORD, and ANDROID_KEY_ALIAS to produce installable end-user APKs." >&2
fi

echo "Final Android artifacts in ${OUT_DIR}:"
ls -lah "$OUT_DIR"
