#!/usr/bin/env bash
# Unit tests for download-binaries.sh helpers — never touches apps/edge/bin.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=download-binaries.sh
source "$SCRIPT_DIR/download-binaries.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Fixture archive + correct/wrong checksums
mkdir -p "$TMP/fixture"
echo "fake-mediamtx-payload" >"$TMP/fixture/mediamtx"
chmod +x "$TMP/fixture/mediamtx"
tar -czf "$TMP/mediamtx_fixture.tar.gz" -C "$TMP/fixture" mediamtx

GOOD_HASH="$(shasum -a 256 "$TMP/mediamtx_fixture.tar.gz" | awk '{print $1}')"
echo "${GOOD_HASH}  mediamtx_fixture.tar.gz" >"$TMP/good.sha256sum"
BAD_HASH="$(echo "$GOOD_HASH" | sed 's/0/1/;s/1/0/')"
# Ensure truly different
if [[ "$BAD_HASH" == "$GOOD_HASH" ]]; then
  BAD_HASH="$(echo "$GOOD_HASH" | sed 's/a/b/')"
fi
echo "${BAD_HASH}  mediamtx_fixture.tar.gz" >"$TMP/bad.sha256sum"

# parse + verify success
PARSED="$(parse_sha256sum_file "$TMP/good.sha256sum")"
[[ "$PARSED" == "$GOOD_HASH" ]] || { echo "FAIL parse good"; exit 1; }
verify_sha256 "$TMP/mediamtx_fixture.tar.gz" "$GOOD_HASH" || {
  echo "FAIL verify good"
  exit 1
}

# mismatch fails
if verify_sha256 "$TMP/mediamtx_fixture.tar.gz" "$BAD_HASH" 2>/dev/null; then
  echo "FAIL expected checksum mismatch"
  exit 1
fi

# Staging install simulation: copy only after good verify; sentinel preserved on fail
DEST="$TMP/dest"
mkdir -p "$DEST"
echo "sentinel" >"$DEST/mediamtx"
SENTINEL_BEFORE="$(cat "$DEST/mediamtx")"

# Bad checksum path must not replace destination
if verify_sha256 "$TMP/mediamtx_fixture.tar.gz" "$BAD_HASH" 2>/dev/null; then
  echo "FAIL"
  exit 1
fi
[[ "$(cat "$DEST/mediamtx")" == "$SENTINEL_BEFORE" ]] || {
  echo "FAIL sentinel mutated on mismatch"
  exit 1
}

# Good path stages into private dest only under TMP
STAGE="$TMP/stage"
mkdir -p "$STAGE"
verify_sha256 "$TMP/mediamtx_fixture.tar.gz" "$GOOD_HASH"
tar -xzf "$TMP/mediamtx_fixture.tar.gz" -C "$STAGE"
cp -f "$STAGE/mediamtx" "$DEST/mediamtx"
chmod +x "$DEST/mediamtx"
[[ -x "$DEST/mediamtx" ]] || {
  echo "FAIL staged binary not executable"
  exit 1
}
[[ "$(cat "$DEST/mediamtx")" == "fake-mediamtx-payload" ]] || {
  echo "FAIL staged content"
  exit 1
}

# Destination paths must live under TMP only
case "$DEST" in
  "$TMP"/*) ;;
  *) echo "FAIL dest outside tmp: $DEST"; exit 1 ;;
esac

# Platform helper returns something
PLAT="$(mediamtx_platform)"
[[ -n "$PLAT" ]] || {
  echo "FAIL platform empty"
  exit 1
}
BASEN="$(mediamtx_archive_basename v1.11.3)"
[[ "$BASEN" == mediamtx_v1.11.3_* ]] || {
  echo "FAIL basename $BASEN"
  exit 1
}

echo "OK test-download-binaries"
