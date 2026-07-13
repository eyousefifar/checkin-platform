#!/usr/bin/env bash
# Minimal ChokePoint setup: fetch and enroll five known identities.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA_DIR="${CHOKEPOINT_DIR:-$ROOT/data/benchmarks/chokepoint}"
API_URL="${API_URL:-http://127.0.0.1:8000}"

fetch() {
  local file marker
  mkdir -p "$DATA_DIR"
  for file in groundtruth.tar.xz P1E.tar.xz P1E_S1.tar.xz; do
    case "$file" in
      groundtruth.tar.xz) marker="$DATA_DIR/groundtruth" ;;
      P1E.tar.xz) marker="$DATA_DIR/P1E_S2_C2" ;;
      P1E_S1.tar.xz) marker="$DATA_DIR/P1E_S1_C1" ;;
    esac
    test -d "$marker" && continue
    curl -fL --continue-at - -o "$DATA_DIR/$file" "https://zenodo.org/records/815657/files/$file"
    tar -xf "$DATA_DIR/$file" -C "$DATA_DIR"
  done
}

enroll() {
  local source="${ENROLL_SOURCE:-$DATA_DIR/P1E_S2_C2}"
  local token response id subject image n image_count
  : "${ADMIN_PASSWORD:?Set ADMIN_PASSWORD to the running edge service password}"
  test -d "$source" || { echo "Missing enrollment faces: $source" >&2; exit 1; }
  token="$(curl -fsS -X POST "$API_URL/api/auth/login" -H 'content-type: application/json' \
    -d "{\"password\":\"$ADMIN_PASSWORD\"}" | sed -nE 's/.*"access_token":"([^"]+)".*/\1/p')"
  test -n "$token" || { echo "Login failed" >&2; exit 1; }

  n=0
  for subject in "$source"/0*; do
    test -d "$subject" || continue
    response="$(curl -fsS -H "authorization: Bearer $token" "$API_URL/api/employees?q=cp-${subject##*/}")"
    id="$(printf '%s' "$response" | grep -oE '"id":[0-9]+' | head -1 | cut -d: -f2)"
    if test -z "$id"; then
      response="$(curl -fsS -X POST "$API_URL/api/employees" -H "authorization: Bearer $token" \
        -H 'content-type: application/json' -d "{\"employee_code\":\"cp-${subject##*/}\",\"full_name\":\"ChokePoint ${subject##*/}\",\"department\":\"benchmark\"}")"
      id="$(printf '%s' "$response" | grep -oE '"id":[0-9]+' | head -1 | cut -d: -f2)"
      test -n "$id" || { echo "Could not create employee for $subject" >&2; exit 1; }
      set --
      image_count=0
      for image in "$subject"/*.pgm; do
        test -f "$image" || continue
        set -- "$@" -F "image=@$image"
        image_count=$((image_count + 1))
        test "$image_count" -ge 5 && break
      done
      test "$image_count" -gt 0 || { echo "No enrollment frames for $subject" >&2; exit 1; }
      response="$(curl -fsS -X POST "$API_URL/api/employees/$id/images" -H "authorization: Bearer $token" "$@")"
      if ! printf '%s' "$response" | grep -q '"embedding_ready":true'; then
        echo "Enrollment has no usable embedding for cp-${subject##*/}: $response" >&2
        exit 1
      fi
      echo "Enrolled cp-${subject##*/} (employee $id)"
    else
      echo "Keeping existing cp-${subject##*/} (employee $id)"
    fi
    n=$((n + 1))
    test "$n" -ge 5 && break
  done
}

case "${1:-}" in
  fetch) fetch ;;
  enroll) enroll ;;
  *) echo "Usage: $0 {fetch|enroll}" >&2; exit 2 ;;
esac
