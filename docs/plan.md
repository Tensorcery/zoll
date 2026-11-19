# zoll - plan of v1

The stages of the first version, in order. Each ends in something a person
can run by hand; a stage is done when every checkpoint under it is ticked.
The decisions behind them are in [design.md](design.md); section numbers
below point there.

A stage gets its own detailed plan, with the tests, when it starts - not
ahead, so the later ones are planned on what the earlier ones taught.

The first usable point is stage 3: from there zoll can replace conftest for
YAML policies.

## 1. Command line and configuration

- [ ] `zoll check`, `inspect`, `lock`, `languages` exist with their flags
      (section 11); each one not built yet says so and exits 2
- [ ] `--version`, `--help`, `--config`, `--color` work
- [ ] the config is found as ruff finds its own: `zoll.toml`, `.zoll.toml`,
      `[tool.zoll]` in `pyproject.toml`, from the working directory up
- [ ] every field of section 9 is parsed; an unknown field or a wrong type
      is an error naming the file, the field and what was expected
- [ ] exit codes 0 / 1 / 2 as section 11 says
- [ ] snapshot tests for each command's output and for each config error

**Checkpoint:** `zoll check` in a repository with a broken `zoll.toml`
prints where it is broken and exits 2; with a valid one it exits 0.

## 2. Files and data formats

- [ ] the walk honours `.gitignore`, `include` and `exclude`, in parallel
- [ ] the language of a file: the config's `languages`, then name and
      extension, then shebang
- [ ] YAML, JSON and TOML become the document conftest gives (section 8.1)
- [ ] a multi-document YAML file is split into its documents
- [ ] a YAML `on:` key stays the string `on`, as in conftest (go-yaml v3),
      not `true` - tested
- [ ] `zoll inspect FILE` prints `input`, `data.conftest.file` and
      `data.zoll.file`
- [ ] the same document as conftest for a corpus of real files, compared in
      a test

**Checkpoint:** `zoll inspect .github/workflows/ci.yml` prints the
document a conftest policy would see.

## 3. Policies and the report

- [ ] regorus at a pinned exact version; policies loaded from directories,
      Rego v1 by default, `rego-version = "v0"` switches the run
- [ ] `data` directories loaded under `data.*`
- [ ] `package <language>` runs on that language; `package main` is an error
      naming the language to use; `[tool.zoll.packages]` binds a package to
      a language; any other package is a library (section 8.4)
- [ ] `deny`, `violation`, `warn` with conftest's name patterns; a string or
      an object with `msg`; conftest `exception` rules (section 8.5)
- [ ] rule ids `package.suffix` (section 8.6)
- [ ] `at` as a path resolved to `line:col` in the YAML/JSON/TOML file;
      no `at` points at the start of the file; `_loc` honoured
- [ ] the `text` report: `path:line:col rule-id: message` with the source
      line; `--fail-on` and `fail-on`
- [ ] the policy engine is compiled once and cloned per file

**Checkpoint:** the owner's conftest policies for workflows and the Helm
chart run through `zoll check` with only `[tool.zoll.packages] main =
"yaml"` added, and report the same violations as conftest, at their lines.

## 4. Code

- [ ] tree-sitter-language-pack compiled in, the base set of section 7
      behind Cargo features, the default feature set = the base set
- [ ] a code file becomes the tree of section 8.2: named nodes, `fields` and
      `children`, `text` on leaves, `start`/`end` in every node
- [ ] columns are 1-based Unicode code points
- [ ] `at` as a node resolves to its position
- [ ] ERROR nodes stay in the tree, typed `ERROR`
- [ ] `zoll languages` lists the compiled-in languages
- [ ] a release build's binary size is measured and written down

**Checkpoint:** a `package bash` policy reports `curl | sh` in a `.sh`
file at its line; `zoll inspect script.sh` shows the tree.

## 5. Embedded code

- [ ] the sync script copies Helix's injection queries first and
      nvim-treesitter's for the languages Helix lacks, at pinned commits,
      strips editor-only directives and lists what it dropped (section 6,
      section 14)
- [ ] the query engine: the captures, properties and predicates of section 14
- [ ] fragments found in data documents and in code trees, to any depth
- [ ] a fragment's text is the decoded value; a segmented map takes its
      positions back through every host to the real file
- [ ] a fragment is evaluated as a file of its own, with `data.zoll.file.host`
      (section 8.3)
- [ ] a fragment whose language is not compiled in: notice `zoll.unchecked`
- [ ] a data file that does not parse: warn `zoll.parse`
- [ ] the license of the wheel and the crates becomes `MIT AND MPL-2.0 AND
      Apache-2.0`, with the license texts shipped

**Checkpoint:** a policy on bash reports a `curl | sh` inside a workflow's
`run:` at its real line in the `.yml`; a policy on awk reports inside the
awk program inside that bash.

## 6. Suppression

- [ ] `per-file-ignores` by glob and rule id
- [ ] `zoll: ignore[rule-id]` in the comment syntax of the real file, on the
      reported line
- [ ] a suppression that matches nothing is reported, so dead ones are found

**Checkpoint:** a violation silenced both ways, and a stale suppression
reported.

## 7. Policies from git and `zoll.lock`

- [ ] `git+<url>//<dir>@<ref>` fetched with the system `git`, shallow, into
      `~/.cache/zoll/policies/`; a cached commit needs no network
- [ ] `zoll.lock` with the commit each source resolved to; runs use it
- [ ] `zoll lock` and `zoll lock --upgrade`
- [ ] a source missing from the lock is resolved and written; `check
      --locked` fails instead
- [ ] a private repository over SSH works with the user's own credentials

**Checkpoint:** `Tensorcery/zoll-policies` (first set `embedded-code`, the
owner's `run:` and `command:` policy) is used by tag; moving the tag does
not change a run until `zoll lock --upgrade`.

## 8. Output formats

- [ ] `--format json`: one object per violation, with the fragment chain
- [ ] `--format github`: workflow commands that annotate the PR diff
- [ ] `--format sarif`: SARIF 2.1.0, `columnKind: unicodeCodePoints`,
      accepted by GitHub code scanning

**Checkpoint:** a PR in a test repository shows zoll's violations as
annotations and in the Security tab.

## 9. Release

- [ ] a CI workflow runs `make lint` and `make test` on every PR
- [ ] wheels built by `maturin-action` for Linux (x86_64, aarch64), macOS
      (x86_64, arm64) and Windows
- [ ] the release workflow publishes to PyPI by Trusted Publishing
- [ ] a pre-commit hook definition (`.pre-commit-hooks.yaml`) for users
- [ ] the README's interface is true: the design status banner goes

**Checkpoint:** `uv tool install zoll-lint` on a clean machine, then
`zoll check` in the owner's repository at work.

## After v1

`zoll-lint[full]` (grammars as shared libraries), user injections in the
config, `--combine` for rules over several files, conftest documents for
Dockerfile, HCL and XML, `zoll test` for policy tests.
