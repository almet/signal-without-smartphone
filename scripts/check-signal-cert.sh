#!/usr/bin/env bash
# Check that the certificates served by Signal's production and staging
# servers still chain to the Signal root CA bundled in the repository.
#
# The tool trusts only that CA (see `build_tls_config` in the core crate), so
# a leaf rotation is harmless as long as the new certificate is issued by the
# same CA. This script catches the day Signal switches to a different one.
#
# Exits 0 when every host verifies, 2 when at least one does not.

set -euo pipefail

CA="crates/signal-setup-core/signal-root-ca.crt"
HOSTS=(chat.signal.org chat.staging.signal.org)

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ca) CA="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

status=0

echo "Pinned CA: $(openssl x509 -in "$CA" -noout -subject -enddate | tr '\n' ' ')"

for host in "${HOSTS[@]}"; do
  output=$(echo | openssl s_client -connect "$host:443" -servername "$host" \
    -CAfile "$CA" -verify_return_error 2>&1 || true)
  verify=$(echo "$output" | grep -E "^ *Verify return code:" | head -1 | sed 's/^ *//')
  leaf_end=$(echo "$output" | openssl x509 -noout -enddate 2>/dev/null | sed 's/^notAfter=//' || true)

  if [[ "$verify" == "Verify return code: 0 (ok)" ]]; then
    echo "$host: chains to the pinned Signal root CA (leaf expires $leaf_end)."
  else
    echo "$host: does NOT verify against the pinned Signal root CA." >&2
    echo "  ${verify:-no verification result (connection failed?)}" >&2
    status=2
  fi
done

exit $status
