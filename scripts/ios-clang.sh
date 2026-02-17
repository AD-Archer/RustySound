#!/usr/bin/env bash
set -euo pipefail

# Keep iOS linking isolated from host/Homebrew include/lib paths.
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
  /usr/bin/clang "$@"
