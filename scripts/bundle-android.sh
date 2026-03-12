#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="${OUT_DIR:-dist/android}"
mkdir -p "$OUT_DIR"

echo "Building Android release bundle via Dioxus..."
dx bundle --platform android --release "$@"

echo "Collecting Android release artifacts..."
mapfile -d '' RELEASE_ARTIFACTS < <(
    find target -type f \
        \( -name "*release*.apk" -o -name "*release*.aab" \) \
        -print0
)

if [ "${#RELEASE_ARTIFACTS[@]}" -eq 0 ]; then
    echo "No Android release artifacts were found under target/." >&2
    echo "Refusing to publish debug artifacts." >&2
    exit 1
fi

for artifact in "${RELEASE_ARTIFACTS[@]}"; do
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
    echo "Signing Android release artifacts..."

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
    for apk in "$OUT_DIR"/*release*.apk; do
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

    JARSIGNER_BIN="$(command -v jarsigner || true)"
    if [ -n "$JARSIGNER_BIN" ]; then
        for aab in "$OUT_DIR"/*release*.aab; do
            signed_aab="${aab%.aab}-signed.aab"
            cp "$aab" "$signed_aab"
            JARSIGNER_ARGS=(
                "$JARSIGNER_BIN"
                -keystore "$KEYSTORE_FILE"
                -storepass "$ANDROID_KEYSTORE_PASSWORD"
            )
            if [ -n "${ANDROID_KEY_PASSWORD:-}" ]; then
                JARSIGNER_ARGS+=(-keypass "$ANDROID_KEY_PASSWORD")
            fi
            JARSIGNER_ARGS+=("$signed_aab" "$ANDROID_KEY_ALIAS")
            "${JARSIGNER_ARGS[@]}"
            rm -f "$aab"
            SIGNED=1
        done
    else
        echo "jarsigner not found; skipping AAB signing." >&2
    fi
    shopt -u nullglob
fi

if [ "$SIGNED" -eq 1 ]; then
    echo "Android release signing complete."
else
    echo "Android release artifacts are unsigned (no signing credentials provided)."
fi

echo "Final Android artifacts in ${OUT_DIR}:"
ls -lah "$OUT_DIR"
