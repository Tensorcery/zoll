#!/usr/bin/env bash
# Writes, next to each file of the corpus, the document conftest parses it
# into; the corpus test compares zoll's documents with these.
#
# conftest: go install github.com/open-policy-agent/conftest@v0.71.1
set -euo pipefail

cd "$(dirname "$0")/../crates/zoll/tests/corpus"
for file in *; do
  case "$file" in
    *.conftest.json) continue ;;
  esac
  # conftest gives a TOML local date in the machine's time zone.
  TZ=UTC conftest parse "$file" >"$file.conftest.json"
done
