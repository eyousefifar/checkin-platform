#!/usr/bin/env bash
# Network/release gate: fetch exact MediaMTX v1.11.3 into a private temp dir.
# Never consults or replaces apps/edge/bin.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=download-binaries.sh
source "$SCRIPT_DIR/download-binaries.sh"

DEST="$(mktemp -d)"
trap 'rm -rf "$DEST"' EXIT

VERSION="v1.11.3"
download_mediamtx "$VERSION" "$DEST"

# Normalize version output to exactly v1.11.3
VER_OUT="$("$DEST/mediamtx" --version 2>&1 || true)"
# MediaMTX prints "v1.11.3" or "mediamtx v1.11.3" depending on build
NORMALIZED="$(echo "$VER_OUT" | tr '[:upper:]' '[:lower:]' | grep -oE 'v?[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
if [[ "$NORMALIZED" != "v1.11.3" && "$NORMALIZED" != "1.11.3" ]]; then
  echo "FAIL version output did not normalize to v1.11.3: $VER_OUT" >&2
  exit 1
fi
# Require exact tag form when prefixed
if [[ "$NORMALIZED" == "1.11.3" ]]; then
  NORMALIZED="v1.11.3"
fi
[[ "$NORMALIZED" == "v1.11.3" ]] || {
  echo "FAIL expected v1.11.3 got $NORMALIZED" >&2
  exit 1
}

case "$DEST" in
  /tmp/*|${TMPDIR:-/tmp}/*) ;;
  *)
    # mktemp may use other private roots; ensure not apps/edge/bin
    case "$DEST" in
      */apps/edge/bin*) echo "FAIL wrote into apps/edge/bin" >&2; exit 1 ;;
    esac
    ;;
esac

echo "OK verify-pinned-download version=$NORMALIZED dest=$DEST"
