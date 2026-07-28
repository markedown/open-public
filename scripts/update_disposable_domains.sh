#!/usr/bin/env bash
# Refresh the vendored disposable-email-domain blocklist from the upstream CC0
# list. Run from the repo root; commit the result. The list is vendored (never
# fetched at runtime) so the server has no network dependency for it.
set -euo pipefail
DEST="crates/server/data/disposable_email_domains.txt"
SRC="https://raw.githubusercontent.com/disposable-email-domains/disposable-email-domains/main/disposable_email_blocklist.conf"
UA="open-public-bot/1.0 (+https://open-public.com; contact via github.com/markedown/open-public)"
tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT
curl -sSL --fail -A "$UA" "$SRC" -o "$tmp"
{
  echo "# Disposable / throwaway email domains, one per line, lowercase, sorted."
  echo "# Source: github.com/disposable-email-domains/disposable-email-domains"
  echo "#   file disposable_email_blocklist.conf, License CC0-1.0 (public domain)."
  echo "# Vendored (not fetched at runtime). Refresh with scripts/update_disposable_domains.sh."
  echo "# Lines starting with # and blank lines are ignored by the parser."
  grep -vE '^\s*(#|$)' "$tmp" | tr 'A-Z' 'a-z' | sed 's/[[:space:]]//g' | grep -E '.\..' | sort -u
} > "$DEST"
echo "wrote $(grep -vcE '^\s*(#|$)' "$DEST") domains to $DEST"
