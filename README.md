# zoll

[Русский](https://github.com/Tensorcery/zoll/blob/main/docs/langs/ru/README.md)

A policy engine for files of any language, conftest-style.

Code hidden inside other code is read by nothing until it runs: awk in a
shell string, a script in a YAML `run:`, a `RUN` line in a Dockerfile, a
`command:` in a Kubernetes manifest. conftest sees data formats only; a
linter sees one language. zoll parses every file - data formats into their
document, code into its syntax tree - finds the code embedded in it, and
runs your Rego policies over all of it.

> **Status: design.** The engine is not written yet; what follows is the
> agreed interface ([docs/design.md](docs/design.md)). Today it runs
> Rego policies over YAML, JSON and TOML, as conftest does, and over the
> tree-sitter tree of code; the code embedded in files comes next.

## How it works

```
.github/workflows/ci.yml          yaml   → package yaml policies
└── jobs.build.steps[0].run       bash   → package bash policies
    └── awk '{print $1}'          awk    → package awk policies
```

1. Each file's language comes from its name, extension or shebang.
2. Data formats (YAML, JSON, TOML…) become the document conftest would give
   your policy - with YAML read as YAML 1.2, so `on:` stays `on`; code
   becomes its tree-sitter tree, as JSON.
3. Code embedded in it is found by where it sits - the argument of `awk`,
   the value of `run:` - and is checked as a file of its own, to any depth.
4. A policy in `package bash` runs on bash wherever it is: a `.sh` file, a
   `run:`, a Dockerfile `RUN`. A conftest policy's `package main` is bound
   to its language by one line of config, and runs with its file untouched.
5. Every violation is reported at its line in the real file.

## A policy

```rego
package bash

# A pipe into a shell downloads and runs code nobody reviewed.
deny_curl_pipe contains {"msg": "curl piped into a shell", "at": node} if {
	walk(input, [_, node])
	node.type == "pipeline"
	some cmd in node.children
	cmd.type == "command"
	cmd.fields.name.text == "curl"
}
```

```
$ zoll check
error[bash.curl_pipe]: curl piped into a shell
  --> .github/workflows/ci.yml:14:11
   |
14 |           curl -fsSL https://example.com/install.sh | sh
   |           ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

1 violation
```

`zoll inspect FILE` prints the JSON a policy sees for that file and every
fragment in it, so a rule is written against the real shape.

## Configuration

`zoll.toml`, `.zoll.toml`, or `[tool.zoll]` in `pyproject.toml`:

```toml
[tool.zoll]
policies = [
  "policy/",
  "git+https://github.com/Tensorcery/zoll-policies//embedded-code@v1.0.0",
]
data = ["policy/data/"]
```

Policies from git are pinned to their commit in `zoll.lock`; `zoll lock
--upgrade` moves them on purpose.

## Install

```bash
uv tool install zoll-lint          # about 25 common languages
uv tool install 'zoll-lint[full]'  # every other grammar zoll has
```

The command is `zoll`; the PyPI name `zoll` is taken by a look-alike.
