# zoll

A policy engine for files of any language: it tells each file's language,
parses it - data formats into the document they hold, code into its tree -
finds the code embedded in it (awk in a shell string, a script in a YAML
`run:`), and evaluates Rego policies over all of it, conftest-style. The
engine holds no opinion: the policies come from the user, or from policy
sets kept in their own repositories. Written in Rust, shipped to PyPI as a binary
wheel by maturin: `uv tool install zoll-lint` installs the `zoll` command
(the PyPI name `zoll` is blocked as too similar to `zoil`).

## Design

`docs/design.md` holds every decision taken before the code: why zoll
exists, what it is not, the pipeline, the languages, the configuration, the
commands. Read it before changing behaviour; a change to a decision is made
there first.

Its section 8, the input model - what `input` and `data.zoll` look like for
a data file, a code file and a fragment, which policies run where, how a
result points at its place, rule ids - is the public API. Every policy
anyone writes depends on it: changing it is a major release.

The pipeline, per file: language → parse (data into its document, code into
its tree-sitter tree) → fragments found by the injection queries → each
fragment through the same steps as a file of its own → the policies of each
language → every violation mapped back to `file:line:col` of the real file.

## Layout

| Path | What it holds |
|---|---|
| `crates/zoll/` | the command: arguments, output; a thin layer over `zoll_core` |
| `crates/zoll_core/` | the engine: files, languages, fragments, policy evaluation |
| `crates/zoll_grammars/` | the grammars with no crate zoll can use, vendored (`vendor/README.md`) |
| `queries/<language>/injections.scm` | where a language holds code of another (tree-sitter queries), copied from Helix by `make queries` |
| `queries/<language>/zoll.scm` | zoll's own injections, where Helix's miss with zoll's grammar |
| `crates/zoll_core/examples/sync_queries.rs` | `make queries`: the copy, and what it leaves out (`queries/README.md`) |
| `crates/zoll/tests/cli/` | the command run as a user runs it; `snapshots/` is insta's expected output |
| `crates/zoll/tests/corpus/` | real files, each with the document conftest parses it into (`*.conftest.json`) |
| `scripts/conftest-corpus.sh` | writes those `*.conftest.json` with conftest |
| `pyproject.toml` | the PyPI wheel (maturin) and the Python tools of this repository |
| `docs/design.md` | the decisions, and the public input model (section 8) |
| `docs/plan.md` | the stages of v1 and their checkpoints; a ticked box is done |
| `docs/langs/ru/` | the Russian README |

Extending zoll is meant to be data more than Rust: an embedding is a query,
a test case is a fixture file. Policies are not part of this repository.

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
yamllint, shellcheck, actionlint, zizmor, pre-commit (jscpd, Regal for any
`.rego` a test uses). It is the pre-push hook, with `make test`.

`make test` - `cargo test`. A test whose snapshot changed fails; `cargo insta
review` shows the difference and takes it.

`make build` - the wheel for this platform, in `dist/`.

## Commits

Conventional Commits, checked by commitizen (`cz.toml`) on every commit. The
version is the workspace's in `Cargo.toml`; `cz bump` raises it there.

A change's code, its tests and its documentation are separate commits, in
that order: first `feat`, `fix` or `refactor` with the code alone; then
`test`; then `docs`. Each later one names in its body the commit it follows,
by its short hash: `Follows 1a2b3c4.`

## Docs

`README.md` is in English, with a Russian copy in `docs/langs/ru/README.md`
kept in step. This file and `AGENTS.md` are in English.
