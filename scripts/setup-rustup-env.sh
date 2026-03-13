#!/usr/bin/env bash
set -euo pipefail

RUSTUP_BIN_DIR="${HOME}/.cargo/bin"

if [[ -d "${RUSTUP_BIN_DIR}" ]]; then
    PATH_WITHOUT_RUSTUP=":${PATH}:"
    PATH_WITHOUT_RUSTUP="${PATH_WITHOUT_RUSTUP//:${RUSTUP_BIN_DIR}:/:}"
    PATH_WITHOUT_RUSTUP="${PATH_WITHOUT_RUSTUP#:}"
    PATH_WITHOUT_RUSTUP="${PATH_WITHOUT_RUSTUP%:}"
    export PATH="${RUSTUP_BIN_DIR}:${PATH_WITHOUT_RUSTUP}"
fi

if command -v rustc >/dev/null 2>&1; then
    RUSTC_PATH="$(command -v rustc)"
    if [[ "${RUSTC_PATH}" != "${RUSTUP_BIN_DIR}/rustc" ]]; then
        cat >&2 <<EOF
warning: rustc resolves to ${RUSTC_PATH}
warning: expected ${RUSTUP_BIN_DIR}/rustc for Rustup-managed Apple targets
warning: prepended ${RUSTUP_BIN_DIR} to PATH for this command
EOF
    fi
fi
