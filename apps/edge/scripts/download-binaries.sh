#!/usr/bin/env bash
# Download MediaMTX (+ optional ffmpeg) into apps/edge/bin for pksp serve.
# Sourcing this file does not install anything — only invoking main does.
# shellcheck shell=bash

# --- side-effect-free helpers (safe to source in tests) --------------------

mediamtx_platform() {
  local OS ARCH
  OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
  ARCH="$(uname -m)"
  local ARCH_TAG MTX_OS
  case "$ARCH" in
    x86_64|amd64) ARCH_TAG=amd64 ;;
    arm64|aarch64)
      # Linux ARM64 uses arm64v8; Darwin ARM64 uses arm64.
      if [[ "$OS" == "linux" ]]; then
        ARCH_TAG=arm64v8
      else
        ARCH_TAG=arm64
      fi
      ;;
    *) echo "unsupported arch: $ARCH" >&2; return 1 ;;
  esac
  case "$OS" in
    darwin) MTX_OS=darwin ;;
    linux) MTX_OS=linux ;;
    *) echo "unsupported os: $OS" >&2; return 1 ;;
  esac
  echo "${MTX_OS}_${ARCH_TAG}"
}

mediamtx_archive_basename() {
  local version="$1"
  local platform
  platform="$(mediamtx_platform)" || return 1
  echo "mediamtx_${version}_${platform}.tar.gz"
}

# Parse a .sha256sum file (GNU/coreutils style: "<hash>  <filename>" or "<hash> *filename").
# Prints the first 64-char hex digest on stdout.
parse_sha256sum_file() {
  local file="$1"
  local line hash
  line="$(head -n 1 "$file" | tr -d '\r')"
  hash="$(echo "$line" | awk '{print $1}')"
  if [[ ! "$hash" =~ ^[0-9a-fA-F]{64}$ ]]; then
    echo "invalid sha256sum content in $file" >&2
    return 1
  fi
  echo "$hash" | tr 'A-F' 'a-f'
}

verify_sha256() {
  local file="$1"
  local expected="$2"
  local actual
  if command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  elif command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$file" | awk '{print $1}')"
  else
    echo "neither shasum nor sha256sum found" >&2
    return 1
  fi
  actual="$(echo "$actual" | tr 'A-F' 'a-f')"
  if [[ "$actual" != "$expected" ]]; then
    echo "checksum mismatch for $file" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    return 1
  fi
  return 0
}

# Download and install MediaMTX into destination directory.
# version: exact tag, e.g. v1.11.3
# destination: directory that will contain the mediamtx executable
download_mediamtx() {
  local version="$1"
  local destination="$2"
  if [[ -z "$version" || -z "$destination" ]]; then
    echo "usage: download_mediamtx <version> <destination>" >&2
    return 1
  fi
  if [[ "$version" == "latest" ]]; then
    echo "refusing to fetch latest; pass an exact tag (e.g. v1.11.3)" >&2
    return 1
  fi
  mkdir -p "$destination"
  local basen url sum_url
  basen="$(mediamtx_archive_basename "$version")" || return 1
  url="https://github.com/bluenviron/mediamtx/releases/download/${version}/${basen}"
  sum_url="${url}.sha256sum"

  local TMP
  TMP="$(mktemp -d)"
  # Caller owns overall cleanup when nested; always clean our temp.
  # shellcheck disable=SC2064
  trap "rm -rf '$TMP'" RETURN

  echo "→ MediaMTX ${version} (${basen})"
  curl -fsSL -o "$TMP/${basen}" "$url" || {
    echo "failed to download $url" >&2
    return 1
  }
  curl -fsSL -o "$TMP/${basen}.sha256sum" "$sum_url" || {
    echo "failed to download checksum $sum_url (missing checksum is fatal)" >&2
    return 1
  }
  local expected
  expected="$(parse_sha256sum_file "$TMP/${basen}.sha256sum")" || return 1
  verify_sha256 "$TMP/${basen}" "$expected" || return 1

  tar -xzf "$TMP/${basen}" -C "$TMP"
  if [[ ! -f "$TMP/mediamtx" ]]; then
    echo "archive did not contain mediamtx binary" >&2
    return 1
  fi
  cp -f "$TMP/mediamtx" "$destination/mediamtx"
  chmod +x "$destination/mediamtx"
  if ! "$destination/mediamtx" --help >/dev/null 2>&1; then
    echo "installed mediamtx is not executable" >&2
    return 1
  fi
  echo "  installed $destination/mediamtx"
  return 0
}

install_ffmpeg_copy() {
  local destination="$1"
  mkdir -p "$destination"
  if ! command -v ffmpeg >/dev/null 2>&1; then
    echo "  WARN: ffmpeg not on PATH — install via brew/apt or place a static binary at $destination/ffmpeg"
    echo "        Copy/transcode publication will be unavailable until then."
    return 0
  fi
  local src
  src="$(command -v ffmpeg)"
  cp -Lf "$src" "$destination/ffmpeg" || {
    echo "failed to copy ffmpeg from $src" >&2
    return 1
  }
  chmod +x "$destination/ffmpeg"
  if ! "$destination/ffmpeg" -version >/dev/null 2>&1; then
    echo "copied ffmpeg is not executable" >&2
    return 1
  fi
  echo "  installed $destination/ffmpeg (from $src)"
  "$destination/ffmpeg" -version | head -1
}

# --- main (only when executed, not sourced) --------------------------------

_pksp_download_binaries_main() {
  set -euo pipefail
  local ROOT BIN MTX_VER
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  BIN="$ROOT/bin"
  mkdir -p "$BIN"
  MTX_VER="${MEDIAMTX_VERSION:-v1.11.3}"
  download_mediamtx "$MTX_VER" "$BIN"
  install_ffmpeg_copy "$BIN"
  echo "Done. pksp serve will auto-discover binaries under bin/."
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  _pksp_download_binaries_main "$@"
fi
