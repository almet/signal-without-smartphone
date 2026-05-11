#!/usr/bin/env bash
# Compare the pinned Signal cert against the one chat.signal.org is serving.
# Exits 0 if they match, 2 if they differ (and writes the new cert + a summary).

set -euo pipefail

PINNED="signal-root.crt"
OUTPUT=""
SUMMARY=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pinned)  PINNED="$2"; shift 2 ;;
    --output)  OUTPUT="$2"; shift 2 ;;
    --summary) SUMMARY="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

OUTPUT="${OUTPUT:-$PINNED}"

fetch_live() {
  echo | openssl s_client -connect chat.signal.org:443 -servername chat.signal.org 2>/dev/null \
    | openssl x509 -outform PEM
}

fingerprint() {
  openssl x509 -in "$1" -noout -fingerprint -sha256 | sed 's/^.*=//'
}

live=$(mktemp)
trap 'rm -f "$live"' EXIT
fetch_live > "$live"

pinned_fp=$(fingerprint "$PINNED")
live_fp=$(fingerprint "$live")

if [[ "$pinned_fp" == "$live_fp" ]]; then
  echo "Pinned Signal cert is up to date ($pinned_fp)."
  exit 0
fi

cp "$live" "$OUTPUT"

report="Signal cert rotation detected.
Previous: $pinned_fp
New:      $live_fp
Written to: $OUTPUT"

echo "$report" >&2
[[ -n "$SUMMARY" ]] && echo "$report" > "$SUMMARY"
exit 2
