#!/usr/bin/env bash
# Publishes dist/ to PyPI by Trusted Publishing, then creates the GitHub
# release v$VERSION with the same files; its tag lands on the commit built.
# A rerun after a failed release finds the files on PyPI and skips them.
#
# The description is what the release pull request said when it was merged
# (prepare-release.sh); without one it is GitHub's list of the PRs merged
# since the previous release.
set -euo pipefail

: "${VERSION:?}" "${GH_TOKEN:?}" "${GITHUB_SHA:?}"

uv publish --trusted-publishing always --check-url https://pypi.org/simple/ dist/*

# Two lines are not for the reader: the note to whoever edits the pull
# request, and the line CodeRabbit was to replace, if it never came.
notes=$(gh pr list --state merged --head "release/v$VERSION" --json body --jq '.[0].body // ""' \
  | tr -d '\r' | sed -e '/^<!--.*-->$/d' -e '/^@coderabbitai summary$/d')

if [ -n "${notes//[[:space:]]/}" ]; then
  gh release create "v$VERSION" dist/* --target "$GITHUB_SHA" --title "v$VERSION" --notes-file - <<< "$notes"
else
  gh release create "v$VERSION" dist/* --target "$GITHUB_SHA" --title "v$VERSION" --generate-notes
fi
