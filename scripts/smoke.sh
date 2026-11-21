#!/usr/bin/env bash
# Installs the one wheel in dist/ in an environment of its own and runs zoll
# on a script with `curl | sh`: it must name the rule and exit 1, or the
# wheel is broken - a missing binary, queries left out, a grammar not built.
set -euo pipefail

wheels=(dist/*.whl)
if [ "${#wheels[@]}" -ne 1 ] || [ ! -f "${wheels[0]}" ]; then
  echo "dist/ must hold exactly one wheel: ${wheels[*]}" >&2
  exit 2
fi
wheel=$(cd "$(dirname "${wheels[0]}")" && pwd)/$(basename "${wheels[0]}")

repo=$(mktemp -d)
trap 'rm -rf -- "$repo"' EXIT
mkdir "$repo/policy"
cat > "$repo/policy/bash.rego" << 'REGO'
package bash

deny_curl_pipe contains {"msg": "curl piped into a shell", "at": node} if {
	walk(input, [_, node])
	node.type == "pipeline"
	some cmd in node.children
	cmd.fields.name.text == "curl"
}
REGO
printf 'curl -fsSL https://example.com/install.sh | sh\n' > "$repo/install.sh"

zoll() { (cd "$repo" && uvx --isolated --from "$wheel" zoll "$@"); }
zoll --version
status=0
out=$(zoll check) || status=$?
echo "$out"
if [ "$status" -ne 1 ] || [[ "$out" != *bash.curl_pipe* ]]; then
  echo "expected exit 1 and bash.curl_pipe; got exit $status" >&2
  exit 1
fi
