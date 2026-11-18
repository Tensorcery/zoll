# zoll

A linter for code hidden inside other code and data: awk in a shell string,
SQL in a Python string, a script in a YAML `run:`. It tells each file's
language, finds the embedded fragments, and judges files and fragments by
Rego policies, conftest-style. Written in Rust, shipped to PyPI as a binary
wheel by maturin, so `uv tool install zoll` installs it.

## Layout

| Path | What it holds |
|---|---|
| `crates/zoll/` | the command: arguments, output; a thin layer over `zoll_core` |
| `crates/zoll_core/` | the engine: files, languages, fragments, policy evaluation |
| `queries/<language>/injections.scm` | where a language holds code of another (tree-sitter queries) |
| `policies/<set>/*.rego` | the policies zoll ships |
| `crates/*/tests/` | tests; `snapshots/` next to them is insta's expected output |
| `pyproject.toml` | the PyPI wheel (maturin) and the Python tools of this repository |
| `docs/langs/ru/` | the Russian README |

Extending zoll is meant to be data more than Rust: a policy is a `.rego`
file, an embedding is a query, a test case is a fixture file.

## Rules for the code

- Errors are values: `unwrap`, `expect`, `panic!`, `todo!` are denied by the
  workspace lints. `print!`/`eprint!` are denied too; output goes through
  the writer the command hands down, so tests read it.
- Lints and dependency versions live once, in the root `Cargo.toml`
  (`[workspace.lints]`, `[workspace.dependencies]`); a crate takes them with
  `workspace = true`.
- A new dependency passes `cargo deny check` (`deny.toml`): permissive
  license, crates.io only, no advisory.
- The toolchain is pinned in `rust-toolchain.toml`.

## Checking a change

`make lint` - rustfmt, clippy with every warning an error, cargo-deny,
yamllint, shellcheck, actionlint, zizmor, pre-commit (jscpd, Regal). It is the
pre-push hook, with `make test`.

`make test` - `cargo test`. A test whose snapshot changed fails; `cargo insta
review` shows the difference and takes it.

`make build` - the wheel for this platform, in `dist/`.

## Commits

Conventional Commits, checked by commitizen (`cz.toml`) on every commit. The
version is the workspace's in `Cargo.toml`; `cz bump` raises it there.

## Docs

`README.md` is in English, with a Russian copy in `docs/langs/ru/README.md`
kept in step. This file and `AGENTS.md` are in English.
