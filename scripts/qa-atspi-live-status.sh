#!/usr/bin/env bash
# Verify that the Orca smoke actually changed Retivus's live status.
# This is an independent AT-SPI inspection, not proof that Orca spoke it.
set -euo pipefail
address="$(busctl --user call org.a11y.Bus /org/a11y/bus org.a11y.Bus GetAddress | sed -n 's/^s "\(.*\)"$/\1/p')"
[[ -n "$address" ]] || { echo 'AT-SPI live-status check: no accessibility bus' >&2; exit 1; }
found=0
nodes=0
scan_status() {
  local service="$1" object="$2" depth="$3" name children child_service child_path
  (( depth <= 12 && nodes < 250 )) || return 0
  nodes=$((nodes + 1))
  name="$(busctl --address="$address" --timeout=2s get-property "$service" "$object" org.a11y.atspi.Accessible Name 2>/dev/null || true)"
  if [[ "$name" == *'Could not open source:'* ]]; then
    found=1
    return 0
  fi
  children="$(busctl --address="$address" --timeout=2s call "$service" "$object" org.a11y.atspi.Accessible GetChildren 2>/dev/null || true)"
  while IFS=$'\t' read -r child_service child_path; do
    [[ -n "$child_service" && -n "$child_path" ]] || continue
    scan_status "$child_service" "$child_path" "$((depth + 1))"
    (( found )) && return 0
  done < <(printf '%s\n' "$children" | grep -oE '"[^"]+"' | tr -d '"' | paste - -)
}
for attempt in 1 2 3; do
  children="$(busctl --address="$address" --timeout=2s call org.a11y.atspi.Registry /org/a11y/atspi/accessible/root org.a11y.atspi.Accessible GetChildren)"
  while IFS=$'\t' read -r service object; do
    [[ -n "$service" && -n "$object" ]] || continue
    scan_status "$service" "$object" 0
    (( found )) && break
  done < <(printf '%s\n' "$children" | grep -oE '"[^"]+"' | tr -d '"' | paste - -)
  if (( found )); then
    echo "PASS: Retivus exposed changed validation status via AT-SPI ($nodes nodes scanned)."
    exit 0
  fi
  sleep 1
done
echo "FAIL: no changed validation status found on AT-SPI ($nodes nodes scanned)" >&2
exit 1
