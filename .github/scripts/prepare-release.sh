#!/usr/bin/env bash
# Bumps the version by commitizen on release/v<next>, pushes the branch and
# opens its PR into $BASE. cz.toml says what a bump touches: Cargo.toml,
# Cargo.lock and CHANGELOG.md, in one commit. The tag is left to release.sh,
# which makes it on the merge commit.
#
# The PR's description is the draft of the release's description: the
# changelog section. What stands there at the merge is what release.sh
# publishes.
set -euo pipefail

: "${BASE:?}" "${GH_TOKEN:?}" "${GITHUB_REF_NAME:?}" "${GITHUB_OUTPUT:?}"
# auto: commitizen takes the increment from the commits since the last tag.
increment=${INCREMENT:-auto}

# The workflow can be started on any branch; a release is cut from one.
if [ "$GITHUB_REF_NAME" != "$BASE" ]; then
  echo "a release is prepared from $BASE, not from $GITHUB_REF_NAME" >&2
  exit 1
fi

# The checkout keeps no token: git asks gh, which has it from GH_TOKEN.
gh auth setup-git
# commitizen reads the whole history and the tags, whatever the checkout fetched.
git fetch --quiet --tags --unshallow 2> /dev/null || git fetch --quiet --tags
git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'

git switch --quiet -c release/next
args=(--yes --changelog)
if [ "$increment" != auto ]; then
  args+=(--increment "$increment")
fi
uv run --locked --only-group dev cz bump "${args[@]}"

version=$(uv run --locked --only-group dev cz version --project)
echo "version=$version" >> "$GITHUB_OUTPUT"
git tag -d "v$version"
git branch -m "release/v$version"
git push --quiet origin "release/v$version"

# The section commitizen has just written for this version.
section=$(awk -v head="## v$version" \
  'index($0, head) == 1 { on = 1; next } on && /^## / { exit } on' CHANGELOG.md)

printf '%s\n' \
  "<!-- This text becomes the description of the release v$version when the pull request is merged. Edit it here. -->" \
  '## Changelog' "$section" \
  | gh pr create --base "$BASE" --head "release/v$version" --title "bump: v$version" --body-file -
