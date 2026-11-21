#!/usr/bin/env bash
# Outputs the version in Cargo.toml and whether it is still to be released:
# new=true when origin has no tag v<version>.
set -euo pipefail

: "${GITHUB_OUTPUT:?}"

version=$(uv run --locked --only-group dev cz version --project)
echo "version=$version" >> "$GITHUB_OUTPUT"

# The version before the first bump is no release.
if [ "$version" = 0.0.0 ]; then
  echo "0.0.0 is not released; prepare release bumps the first version"
  echo "new=false" >> "$GITHUB_OUTPUT"
  exit 0
fi

# 0: the tag is there. 2: it is not. Anything else is a failure to ask, and
# must not pass for "not released yet".
found=0
git ls-remote --exit-code --tags origin "refs/tags/v$version" > /dev/null || found=$?
case $found in
  0) echo "v$version is released already"; echo "new=false" >> "$GITHUB_OUTPUT" ;;
  2) echo "new=true" >> "$GITHUB_OUTPUT" ;;
  *) echo "could not ask origin for the tag v$version" >&2; exit "$found" ;;
esac
