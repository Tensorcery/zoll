# zoll - design

Status: agreed in discussion on 2026-11-19, before any engine code. The
sections marked *public API* are what policies and users build on; changing
them is a major release. Everything else may change between versions.

## 1. Why

Code hidden inside other code or data is read by nothing until it runs: awk
in a shell string, a script in a YAML `run:`, a `RUN` line in a Dockerfile, a
container's `command:` in a Kubernetes manifest. No linter and no type
checker sees it; a lost space, a quote, a status swallowed by a pipe surface
on the runner or in the cluster.

Today the owner closes part of this with conftest: a Rego policy refuses a
`run:` longer than one line, so the code has to move to a file that
shellcheck reads. conftest only sees data formats, though: inside a `.sh`
file, or inside the string a YAML field holds, it sees nothing.

zoll is a policy engine for files of any language. It tells each file's
language, parses it - data formats into the document they hold, code into
its syntax tree - finds the code embedded in it, and evaluates Rego
policies over all of it, conftest-style.

## 2. Goals, non-goals, anti-goals

Goals:

- Any file, any language: data formats as conftest reads them, code through
  tree-sitter, and the code embedded in either, to any depth.
- Rego policies; an existing conftest policy runs with no edit to its file
  (one line of config binds its package to a language, 8.4).
- Every violation printed as `file:line:col` of the real file, also when it
  sits in a fragment inside a fragment.
- Fast: in-process parsing and evaluation, parallel over files, no process
  spawned per file. It runs in pre-commit and on every push.
- Installed by `uv tool install zoll-lint` (command `zoll`), one binary.

Non-goals:

- Semantic code analysis: data flow, taint, types, call graphs. Semgrep and
  the language's own linters own that; zoll does not compete there.
- Shipping opinions. The engine holds no policies; policy sets live in their
  own repositories (first one: `Tensorcery/zoll-policies`, set
  `embedded-code`).
- Fixing code. zoll reports; it does not rewrite.
- Guessing a string's language from its content. An embedded fragment is
  found by where it sits, never by what it looks like.

Anti-goals - it fails even if it works when:

- an existing conftest policy needs its file edited to run;
- a violation points at a line inside a fragment instead of the real file;
- adding a language means writing Rust for that language;
- a policy set changes under a user without a visible diff in their
  repository.

## 3. Where it stands among existing tools

- conftest, Trivy, Checkov, Regula: policies over data formats and IaC. They
  do not parse code; a `RUN` line or a `run:` value is a string to them.
- Semgrep / Opengrep, ast-grep, GritQL: many languages, embedded code
  (Semgrep's `metavariable-pattern` with `language`, ast-grep's
  `languageInjections`), but rules are code patterns in their own DSL, not
  Rego, and free Semgrep sees one file at a time.
- goast: Go AST as JSON under Rego - one language, its own parser.

zoll's place: one policy language, Rego, for configs, for code, and for the
code inside configs; conftest-compatible; facts from external data
combined in one rule, and later from several files (a `--combine` mode,
section 15). Rules about code alone ("no `eval`")
stay with Semgrep and the language linters.

## 4. Approaches considered

| Approach | Trade-off | Outcome |
|---|---|---|
| Rust, tree-sitter in process, regorus (Rego in Rust) in process | fastest, one binary, maturin to PyPI like ruff/uv/zizmor; regorus is 0.x and not the reference OPA | **chosen** |
| Go, reference OPA, tree-sitter through cgo | full OPA compatibility; cgo complicates cross-builds and the wheel | rejected |
| Python, tree-sitter-language-pack, `opa` as a subprocess | simplest to write; a process per file and tree-to-JSON in Python are too slow for a pre-commit tool | rejected: speed |
| Grammars downloaded at first run (language-pack's default) | small binary; network at runtime, the cache becomes a security boundary | rejected; extra grammars come as PyPI packages instead (section 7) |

## 5. Architecture

Pipeline, per file:

1. Language: from the config's `languages` table (the first glob written
   that matches), then the file's whole name, its extension, the name
   before its first dot (`Dockerfile.dev`; never for a `.lock` file), then
   the first line's shebang.
2. Parse: a data format into its document; code into a tree-sitter tree.
3. Fragments: the injection queries of that language find embedded code;
   each fragment gets a language and a byte range in its host.
4. Recurse: each fragment goes through steps 2-4 as a file of its own.
5. Evaluate: the policies that apply to the language (section 8.4) run on
   the file and on each fragment.
6. Report: every result is resolved to `file:line:col` of the real file.

Files are processed in parallel. Policies are compiled once into a regorus
`Engine`; each evaluation clones it and sets its own `input` and `data`.

Crates (zizmor's shape: few, small):

| Crate | Holds |
|---|---|
| `crates/zoll` | the command: arguments, output formats |
| `crates/zoll_core` | the engine: config discovery, walking, languages, parsing, fragments, locations, evaluation, lock |

Repository data, read at build time:

| Path | Holds |
|---|---|
| `queries/<language>/injections.scm` | where a language holds code of another, copied from Helix (section 6) |
| `queries/<language>/zoll.scm` | zoll's own, where Helix's miss with zoll's grammar |
| `queries/languages.toml` | Helix's `injection-regex` of each language, for the names injections give |

A third crate appears only when something earns it (a SARIF model, test
helpers), not ahead of time.

## 6. Injections: the map of embedded code

An injection is a tree-sitter query that says: in language X, at this node,
sits code of language Y. Examples: the argument of `awk` in bash is awk; the
value of `run:` in YAML is bash; a fenced block in Markdown is the language
of its info string.

- Source: editor query collections, not written by hand. nvim-treesitter
  (Apache-2.0, ~300 files) and Helix (MPL-2.0, 232 files) are copied into
  `queries/` by a sync script at a pinned commit. Each file keeps its license
  header; nvim parts inside Helix files keep their attribution. Updating is
  raising the commit and re-running the script; the diff is reviewed.
- The sync script strips editor-only directives zoll does not evaluate
  (e.g. Neovim's `#offset!`). It leaves out an injection that names a
  language zoll does not have (`comment`, `jsdoc`, Lua patterns), uses a
  predicate zoll does not evaluate, or does not compile against the grammar
  zoll is built with, and lists each.
- An injection Helix's queries miss with zoll's grammar is written by zoll in
  `queries/<language>/zoll.scm`, read after Helix's file: a Dockerfile `RUN`
  (tree-sitter-containerfile has no `shell_fragment` node), a `just` recipe
  without a shebang. YAML's file is zoll's alone, and the sync copies none
  for it: Helix's misses `run:` as a quoted scalar or in a flow mapping, the
  `commands:` lists and the script after `-c` in an argv list.
- A later match of the same text replaces an earlier one only when the
  earlier was a guess and the later names its language (a C++ file's C
  query loses to its C++ one).
- An injection that names its language by a capture (a Markdown fence's
  info string) is matched with zoll's names first, then with Helix's
  `injection-regex` of each language, the whole name; `sync` copies those
  into `queries/languages.toml`. A name no language has (`text`, `console`)
  is not a fragment.
- Semantics follow Helix's tree-house: captures `@injection.content`,
  `@injection.language`, `@injection.filename`, `@injection.shebang`;
  properties `injection.language`, `injection.combined`,
  `injection.include-children`, `injection.include-unnamed-children`. An
  unknown predicate fails the query load.
- Coverage is never total: a string whose language no context reveals is
  not a fragment. zoll writes only the gaps (e.g. SQL in Python strings) and
  the tests.
- The editor maps also mark comments and regexes. Regexes are kept (regex is
  in the base set); comments are not, since zoll has no grammar for them.
  Whether to limit which languages become fragments is decided on real files
  later.
- User-defined injections in the config are not in v1.

## 7. Languages and distribution

- Grammars are compiled in, never downloaded. Each comes from its own crate
  on crates.io (`tree-sitter-bash`, `tree-sitter-python`, ...), pinned in
  `Cargo.lock` and checked by `cargo deny`; a grammar with no crate zoll can
  use (none under a permissive license, or one that binds an older
  tree-sitter) is vendored in `crates/zoll_grammars/` with its source and
  revision: awk, gotmpl, jq, just, perl, scss. dockerfile is `tree-sitter-containerfile`, sql is
  `tree-sitter-sequel`, markdown is the block grammar of `tree-sitter-md`.
  tree-sitter-language-pack was the first idea and was dropped: its build
  downloads the grammar sources from a release and checks them against a hash
  from the same server.
- YAML, JSON and TOML are read as data (8.1) and are always there. YAML's
  grammar (`tree-sitter-yaml`) is always compiled in as well: its tree is
  where the injection queries find a workflow's `run:`.
- Each language is a Cargo feature of its name; `base` is the set below and
  is the default.
- `zoll-lint` (PyPI) carries the binary with the base set, 50 languages:
  - data and config: yaml, json, toml, xml, hcl, dockerfile, ini, nginx,
    proto, graphql, jsonnet, nix;
  - build: make, cmake, starlark (Bazel, Tilt), just, groovy (Jenkinsfile);
  - templates: gotmpl (Helm charts are Go templates);
  - shell and the languages shell holds: bash, powershell, awk, jq, perl,
    lua;
  - web: javascript, typescript, tsx, html, css, scss, svelte;
  - general: python, go, rust, java, kotlin, scala, c, cpp, c_sharp, php,
    ruby, swift, dart, elixir, haskell, zig;
  - other: sql, markdown, regex (for the code embedded in others).
- Left for `full` as too heavy for what they bring here: ocaml (12 MiB),
  julia (6), objc (5), and r, erlang, solidity.
- `zoll-lint[full]` adds one package with every other grammar as shared
  libraries installed under `<prefix>/share/zoll/grammars/`. The binary,
  at `<prefix>/bin/zoll`, looks in `../share/zoll/grammars/` at start. If one
  wheel exceeds PyPI's 100 MB file limit, `full` is split into a few packages
  the extra pulls together, or a limit increase is requested.
- A custom set is a build from git, `cargo install --git
  https://github.com/Tensorcery/zoll zoll` with Cargo features
  naming the languages. zoll is not published on crates.io: `queries/` sits
  outside the crates, and a published crate carries only its own directory.
- `zoll languages` lists what this installation can parse.
- The release binary with the base set is 70 MiB (73,241,040 bytes; 8.0 MiB
  gzipped), 7.6 MiB without any grammar (x86_64 Linux, rustc 1.100, thin LTO,
  stripped). The grammars are most of it: sql 12.7 MiB, c_sharp 6.9, scala
  4.3, swift 3.9, haskell 3.7, kotlin 3.3, cpp 3.2, typescript and tsx 2.7;
  most of the rest are under 1 MiB each. With the first 30 languages it was
  54 MiB.

## 8. The input model *(public API)*

### 8.1 Data formats

YAML, JSON and TOML: `input` is the parsed document, as conftest gives it:
no key added. A multi-document YAML file is evaluated one document at a
time, and so is each item of a root array (YAML or JSON), as conftest
evaluates them. `data.conftest.file` is set as conftest sets it: `name` the
file name, `dir` its absolute directory.

Where zoll differs from conftest, on purpose (owner's decision, 2026-11-19):

- **YAML is YAML 1.2.** conftest reads YAML through `sigs.k8s.io/yaml`,
  which is go-yaml v2 and YAML 1.1: GitHub's `on:` becomes the key `true`,
  and `yes`, `no`, `y`, `n`, `on`, `off` become booleans. zoll keeps them
  strings, as GitHub Actions and Kubernetes read the file. A conftest policy
  that wrote `input["true"]` for a workflow's triggers writes `input.on`.
  Merge keys (`<<`) are honoured, as GitLab CI and Compose files use them.
- **A TOML date or time is the string TOML wrote it as.** conftest turns a
  local one into the machine's time zone, which no policy can rely on.

Like conftest, zoll refuses what JSON has no room for: `.inf` and `.nan`, a
null key, a list or a mapping as a key. A later duplicate key wins, as in
conftest. `tests/corpus/` holds real files with the documents conftest
parsed them into, and a test compares them with zoll's.

Dockerfile, HCL and XML are parsed as code in v1 (8.2): conftest builds
their documents with Go libraries in shapes of their own, and re-creating
them is later work. A conftest policy for those formats does not run until
then.

### 8.2 Code

`input` is the tree-sitter tree as nested JSON:

- named nodes only (punctuation and anonymous nodes are left out);
- each node: `type`, `text`, `start` and `end` (`{line, column}`,
  1-based), `fields` for children reached by a field name, `children` for
  the others;
- `text` is on every node, not the leaves only: a policy reads
  `cmd.fields.name.text` without knowing that bash's `command_name` holds a
  `word`. The text of every node adds 6 to 11 times the file's size; the
  tree without it, as `zoll inspect` prints it, is already 120 to 190 times
  the file (measured at stage 4);
- a field that may hold several nodes (`multiple` in the grammar's
  `node-types.json`) is always a list, even with one node, so a policy reads
  it the same way in every file; any other field is an object;
- a part that does not parse is a node of type `ERROR` and stays in the
  tree; a node the parser inserted for what the text lacks (`MISSING`) is
  left out, as nothing in the file is there to point at;
- columns count Unicode code points, as everywhere in zoll;
- a tree nested deeper than 512, whose text is more than 64 times the
  file's size (at least 1 MiB is allowed), or past a million nodes, is
  refused: the file gets a `zoll.parse` warning. Text on every node would
  otherwise let a deep file copy itself hundreds of times over, and a node
  takes about 3 KB, so a dense file of `a;` would ask for gigabytes. The
  budget is the file's: its fragments' trees (8.3) spend from it too, and
  one that overruns it is refused with the same warning.

```json
{"type": "program", "text": "ls | awk '{print $1}'\n",
 "start": {"line": 1, "column": 1}, "end": {"line": 2, "column": 1},
 "children": [
  {"type": "pipeline", "text": "ls | awk '{print $1}'", "start": {...}, "end": {...}, "children": [
    {"type": "command", "text": "ls", "start": {...}, "end": {...},
     "fields": {"name": {"type": "command_name", "text": "ls", "start": {...}, "end": {...},
                         "children": [{"type": "word", "text": "ls", "start": {...}, "end": {...}}]}}},
    {"type": "command", "text": "awk '{print $1}'", "start": {...}, "end": {...},
     "fields": {"name": {"type": "command_name", "text": "awk", "start": {...}, "end": {...},
                         "children": [{"type": "word", "text": "awk", "start": {...}, "end": {...}}]},
                "argument": [{"type": "raw_string", "text": "'{print $1}'", "start": {...}, "end": {...}}]}}]}]}
```

Positions are inside the nodes here, unlike data formats: this format is
zoll's own, so nothing breaks, and a policy can point at a node (8.5).

Node names are the grammar's own; `zoll inspect` shows them.

### 8.3 Fragments are files

An embedded fragment is evaluated as a file of its own: its `input` is its
own document or tree. A policy about bash runs on `.sh` files, on bash in a
`run:`, and on bash in a Dockerfile `RUN` alike.

`data.zoll.file` tells what is being evaluated:

```json
{
  "path": ".github/workflows/ci.yml",
  "language": "bash",
  "host": {
    "path": ".github/workflows/ci.yml",
    "language": "yaml",
    "at": ["jobs", "build", "steps", 0, "run"],
    "document": 0
  }
}
```

A file has no `host`. A data host names the `document` the fragment is in,
counted from 0 as the file's evaluations are: the document of a YAML
stream, or the item of a root array, whose `at` is then from that item. A fragment's `host.at` is a path when the host is a
data document and a node when the host is code: its `type`, `text`, `start`
and `end` as in 8.2, without the nodes under it. A fragment in a fragment
names the fragment as its host, and that host has its own `host`, down to
the file. zoll's own facts live under `data.zoll`, never inside `input`.

A fragment's `input` is built from its decoded text (section 14): the
`start` and `end` of its nodes count lines and columns in that text, and
zoll maps a result's place back to the file.

### 8.4 Which policies run where

The Rego package says the language: policies in `package bash` run on bash
only (files and fragments), `package yaml` on YAML only; the package name is
zoll's name of the language, as `zoll languages` lists it.

A package that names no language is a library: it can be imported, it is
never evaluated on its own. `package main` is an error unless the config
binds it - zoll stops and says which language to name instead:

```
policy/workflows.rego: package main does not name a language;
use the language it checks, e.g. `package yaml`
```

`[tool.zoll.packages]` binds a package to a language, so conftest policies
(`main`, `kubernetes`, `docker`...) run with their files untouched:

```toml
[tool.zoll.packages]
main = "yaml"
kubernetes = "yaml"
```

Starting strict keeps the door open: allowing more later breaks no one,
narrowing a permissive rule would.

### 8.5 Results and where they point

A `deny` or `warn` rule (conftest's rule-name patterns:
`^(deny|violation)(_[a-zA-Z0-9]+)*$`, `^warn(_[a-zA-Z0-9]+)*$`) yields:

- a string - the violation points at the start of the file or fragment;
- `{"msg": ..., "at": [path]}` - a path in a data document, resolved to its
  position;
- `{"msg": ..., "at": node}` - a code node; its `start`/`end` are used.

Positions in fragments are mapped back through every host to the real file.
Other keys in the object are kept as metadata, as conftest does. conftest's
`_loc` is honoured when present.

### 8.6 Rule ids

A rule's id is the package and the rule-name suffix: `deny_curl_pipe` in
`package bash` is `bash.curl_pipe`; a bare `deny` in `bash` is `bash`. Ids
are what suppression, the reports and SARIF's `ruleId` use.

## 9. Configuration

Found as ruff finds its own: `zoll.toml`, `.zoll.toml`, or `[tool.zoll]` in
`pyproject.toml`, searched from the working directory up. `--config` names
one; its globs and paths are from its own directory, as a found config's
are, and its `zoll.lock` sits beside it. In one directory `.zoll.toml` comes before `zoll.toml`, and both before
`pyproject.toml`; a `pyproject.toml` without `[tool.zoll]` is passed over.
`zoll.toml` and `.zoll.toml` hold the fields at the top level, without the
`[tool.zoll]` header the example below has.

```toml
[tool.zoll]
include = ["**/*"]
exclude = ["vendor/**"]            # .gitignore is always honoured; .ignore
                                   # and one machine's git excludes are not

policies = [
  "policy/",                                                            # a directory
  "git+https://github.com/Tensorcery/zoll-policies//embedded-code@v1.0.0",
  "git+ssh://git@github.com/my-org/policies//k8s@3f2a9c1e8b0d4c6a5f7e9b2d1c3a4e5f6b7c8d9e", # a commit
]
data = ["policy/data/"]           # JSON/YAML under data.*, as conftest --data

fail-on = "deny"                  # or "warn": warnings fail too

[tool.zoll.packages]              # a package that is not a language name (8.4)
main = "yaml"

[tool.zoll.languages]             # a file whose name does not tell its language
"Jenkinsfile" = "groovy"

[tool.zoll.per-file-ignores]
"scripts/legacy/**" = ["bash.curl_pipe"]
```

A path given to `zoll check` is cleaned of `.` and `..` before the globs
see it, so `zoll check ..` from a subdirectory is the project as `zoll
check` at its root is.

`policies` is `["policy/"]` when the config names none, as conftest's
`--policy` default. A data file's document goes at the path of its
directory under the data directory, its file name left out, as conftest's
`--data` puts it: `policy/data/teams/owners.yaml` holds `data.teams.*`.

An inline suppression: a comment `zoll: ignore[bash.curl_pipe]` on the line
the violation points at, in whatever comment syntax that file's language
has; several ids go in one comment, `zoll: ignore[bash.curl_pipe, bash.eval]`.
Rule ids are matched whole. A suppression that silences nothing is a warning,
`zoll.unused_ignore`, so a dead one is found and removed; it is not silenced
itself.

## 10. Policies from git and `zoll.lock`

- A source is `git+<url>//<directory>@<tag-or-commit>`. The `//` is the
  first one after the URL's own `scheme://`; the ref is after the last `@`.
  A commit is named in full: git fetches no commit by a prefix of its id. The
  directory stays inside the repository (no `..`, not absolute).
- Fetched with the system `git`, so every credential the user has for git
  works (SSH agent, credential helpers, `gh auth`). Shallow, the one commit,
  into `~/.cache/zoll/policies/<commit>`, the commit's tree without its
  `.git`; a cached commit needs no network. The cache is `$ZOLL_CACHE_DIR`
  when set, else `zoll/policies` under `$XDG_CACHE_HOME` or `~/.cache`; a
  relative `$ZOLL_CACHE_DIR` is refused, a relative `$XDG_CACHE_HOME`
  passed over. A fetch a killed run left behind is removed after a day.
- git runs with no terminal to prompt on and none of the `GIT_*` variables
  that name another repository (a hook's `GIT_DIR`), and is stopped after
  `$ZOLL_GIT_TIMEOUT` seconds, 600 by default. A password in a source's URL
  is printed as `***`. A source's directory that is a link out of its
  commit's tree is refused.
- `zoll.lock`, beside the config, records per source the commit its ref
  resolved to. Runs use the locked commit, never the ref's current target. It
  is TOML: `version = 1`, and a table `policies` from each source, as the
  config writes it, to its commit. A config with no git source has no lock.
- `zoll lock` writes the lock for sources missing from it;
  `zoll lock --upgrade` moves every ref to its current commit. Each prints
  the sources it moved, one line each: the source and the commit's first 12
  characters. A source the config no longer names leaves the lock.
- `zoll check --locked` fails (exit 2) when the lock and the config disagree:
  a source the lock lacks, or one the config dropped.

## 11. Command line

```
zoll --version | -V
zoll --help | -h            each command has its own --help
  --config PATH             this config instead of the search
  --color auto|always|never

zoll check [PATHS...]       default: the current directory
  --format text|json|github|sarif
  --fail-on deny|warn
  --locked
  --show-files              list each file to check and its language, check none

zoll check -                the standard input, read as YAML, as conftest reads
                            it: `helm template . | zoll check -`
zoll inspect FILE           input and data of the file and of each fragment
zoll lock [--upgrade]
zoll languages
```

A file named in `PATHS` is checked even when `include` or `exclude` would
leave it out, as ruff does. `include`, `exclude` and the `languages` table
are globs from the config's directory; `*` stays inside a directory and
`**` crosses them, and a `languages` glob without a `/` matches the file
name wherever the file is.

`zoll inspect` prints a JSON array, one object per evaluation: `{"input":
..., "data": {"conftest": {"file": ...}, "zoll": {"file": ...}}}`.

Exit codes: `0` no violation, `1` violations, `2` zoll itself failed
(config, network, a policy that does not load).

Later: `zoll test` for policy tests (`*_test.rego`), user injections in the
config.

## 12. Output

- `text`: each violation as rustc draws a diagnostic, then a count:

  ```
  error[yaml.curl_pipe]: curl piped into a shell
   --> .github/workflows/ci.yml:14:9
     |
  14 |         run: |
     |         ^^^
  ```

  `error` for `deny` and `violation`, `warning` for `warn`. A result with
  `at` points at that path's key (a list item: the item), or at the nearest
  of its parents that is in the file; without `at`, at the start of its
  document - a string result cannot say more, so a policy returns `at` to be
  shown at its line. conftest takes the same object and keeps `at` as
  metadata, so one policy serves both.
- `json`: an array, one object per violation - `rule`, `severity` (`deny`,
  `warn` or `notice`), `message`, `path`, `start` and `end` (each `line` and
  `column`), `fragments`, and `metadata` when the result had other keys.
  `fragments` is the chain it was found through, the outermost first, each
  `{"language", "at"}` with `at` where the fragment sits in its host, as
  `data.zoll.file.host.at` has it (8.3); empty for a result on the file.
- `github`: one workflow command per violation, so violations annotate the
  PR diff: `::error file=…,line=…,col=…,endLine=…,endColumn=…,title=<rule>::<message>`,
  `warning` for `warn` and `notice` for a notice, escaped as GitHub escapes.
- `sarif`: SARIF 2.1.0 for GitHub code scanning, built as JSON with only the
  fields zoll emits: one run, the rules that have results in it, each result
  with its `ruleId`, `level` (`error`, `warning`, `note`), message and region;
  `columnKind: "unicodeCodePoints"`, `endColumn` past the last character.
  The path is relative to the working directory, percent-encoded as a URI;
  a file outside it is a `file://` URI of its absolute path.

`path` is the path as the text report shows it: relative to the working
directory when the file is under it.

## 13. Risks

- regorus is pre-1.0 with frequent breaking minor releases: the exact
  version is pinned. A CI job that runs a corpus of real conftest policies
  through OPA and zoll and compares comes with CI itself (plan.md, release).
- regorus lacks some builtins (`rego.metadata.*`, `jwt.*`, some
  `net.cidr_*`, `rego.parse_module`, `strings.render_template`) and rejects
  `not x = v`; a conftest policy using them does not load. Regal, run on
  policies, does not know regorus' gaps; a capabilities file for Regal that
  removes them is to be published with the release (plan.md).
- Editor injection queries are written for highlighting; they mark more
  than zoll needs, and their predicates must be supported or stripped.
- MPL-2.0 query files stay files of their own under that license.
- The `full` grammar package may exceed PyPI's size limit (section 7).

## 14. Calls made after the design review

A reviewer read this document as the builder would. These answers were not
discussed with the owner; they are the author's calls, open to correction.

- **Fragment text is the decoded value**, the text the target language's
  interpreter receives: a YAML scalar's value (escapes resolved, block
  indentation removed), a shell string without its quotes, a Dockerfile
  `RUN` with its line continuations joined, the joined ranges of an
  `injection.combined`. Positions map back through a segmented map (each
  decoded run → its range in the host), not one offset.
- **Query sources**: Helix first, since its semantics are the ones zoll
  implements; nvim-treesitter only for a language Helix lacks. One source per
  language, never merged. zoll implements the predicates the kept files use -
  at least `#eq?`, `#match?`, `#any-of?`, `#not-*` forms, and Markdown's
  info-string language. nvim's `#lua-match?` is not: no kept file uses it,
  and a file taken from nvim-treesitter would bring it with it. An injection that needs a predicate zoll cannot
  evaluate is dropped by the sync script and listed in its output.
- **A fragment whose language is not installed** is reported as a notice
  (`zoll.unchecked`, printed, never a failure), so a user knows that code was
  not checked. So is a fragment that does not parse as its language (found
  while building stage 5): the body of Rust's `json!` holds Rust
  expressions, a Markdown example is cut short with `...`; the guess was the
  host's, and a warning on each would be noise. This is data only: code
  always parses, and a fragment's tree zoll refuses (too deep, past the
  file's text budget) is `zoll.parse`, a warning, as a file's is. A file
  that does not parse stays `zoll.parse` too. A fragment nested more than 8
  fragments deep is not looked into, a notice.
- **A file zoll cannot read fails the run** (after the roast): one that is
  not UTF-8, or over 16 MiB, is `zoll.parse` at deny. Code always parses,
  so a warning would let one byte of Latin-1 take a script out of every
  policy with the run still green. The other files are still checked;
  `per-file-ignores` passes such a file on purpose.
- **Data limits.** YAML and JSON nest at most 128 deep, serde_json's limit
  (tree-sitter-yaml's scanner aborts the process not far past it); YAML's
  anchors and aliases may build at most a million nodes and 64 times the
  file's text.
- **What is decoded** (stage 5): a YAML block scalar (its header gone, its
  indentation removed, folded and chomped as its indicators say), a YAML
  plain or quoted scalar (its lines folded, its escapes resolved), a shell
  string in single quotes (without them), the string literal a captured
  piece is part of in Rust, Python, Ruby, bash, Go, JavaScript and
  TypeScript (whole, escapes resolved, an interpolation as `zoll`), and a
  `//go:generate` comment as its command. A workflow step's `run` is the
  language its `shell` (or the job's or workflow's `defaults.run.shell`)
  names, past wrappers such as `env`, `nice`, `sudo` and `timeout` and the
  flags each takes; past a flag zoll does not know, it stays bash, so
  bash's policies still see it rather than a guess taking it from them. Other content is the node's text, less its children unless the
  pattern includes them. The matches of an `injection.combined` pattern are
  one fragment, a line break between them unless the host continues the
  line (`\` at its end).
- **Helix only, for now** (stage 5): Helix has a file for every language of
  the base set that has one, so nvim-treesitter is not copied; it stays the
  source for a language Helix lacks.
- **Parse failures.** tree-sitter always yields a tree: ERROR nodes stay in
  it, typed `ERROR`, and policies see them. A data format that does not
  parse (Helm templates, `${{ }}` that breaks YAML) is a violation with the
  reserved id `zoll.parse`, severity warn; `per-file-ignores` silences it for
  templated paths.
- **OPA's builtins, less two.** regorus is built without `json.match_schema`
  (its JSON Schema stack is licensed MIT-0) and without `http.send`: a
  policy reads the files it is given, not the network.
- **Rego v1 by default** (conftest's and regorus' default); `rego-version =
  "v0"` in the config switches the whole run, since regorus sets it per
  engine.
- **conftest `exception` rules** are honoured as conftest honours them.
- **A source missing from `zoll.lock`**: `zoll check` resolves it and writes
  the lock, as `uv run` does, so the change shows in `git diff`; with
  `--locked` it fails instead.
- **The cache is keyed by the commit alone** (stage 7), not by repository
  and commit: a commit id names its tree, whichever URL it came from, so two
  sources of one commit share it and no URL needs escaping into a path.
- **A commit is named in full** (stage 7): a shallow fetch takes a ref or a
  full id, never a prefix, and resolving a prefix would need the whole
  history.
- **Inline suppression in a fragment** is written in the real file, on the
  line the violation is reported at, in that file's comment syntax; the same
  line only. A host with no comments (JSON) has `per-file-ignores` only.
- **Which comments count** (stage 6): `zoll: ignore[...]` after any common
  comment opener (`#`, `//`, `--`, `;`, `%`, `/*`, `<!--`), not the one
  opener of each language. One rule serves the host and the fragment alike:
  bash in a YAML `run:` takes `#` either way, and a Markdown fence takes its
  own language's. A marker inside a string counts too; whoever can write it
  there can write a comment.
- **An unused `per-file-ignores` entry** (stage 6) is reported only by a run
  over the whole project, a path given that holds the config's directory:
  a run over some files (a pre-commit hook's, on the files changed) has not
  seen the files a glob was written for. An unused comment is judged in its
  own file, always, unless the file did not parse.
- **Columns are 1-based Unicode code points** (as ruff counts them). SARIF
  says so with `columnKind: "unicodeCodePoints"`.
- **`zoll-lint[full]` comes after v1.** Loading a grammar from a shared
  library needs `unsafe`; the workspace lint becomes `deny`, and the one
  loading module allows it with a comment. v1 ships the base set only.

## 15. Decided after the review, with the owner

1. `package main` is an error; `[tool.zoll.packages]` binds a package to a
   language; any other package is a library (8.4).
2. v1 gives conftest's documents for YAML, JSON and TOML; Dockerfile, HCL
   and XML are code until their conftest shapes are built (8.1).
3. Cross-file rules (conftest `--combine`, all files as one `input`) are not
   in v1. They come later as a mode switched on explicitly, a new input shape
   beside the per-file one, so adding them breaks no policy.
4. YAML is read as YAML 1.2, not as conftest's YAML 1.1: `on:` stays `on`
   (8.1). Found while building stage 2; the plan had assumed conftest used
   go-yaml v3.
5. A root array is evaluated item by item, as conftest does, in YAML and
   JSON alike (8.1).
6. Grammars come from their own crates on crates.io, and awk and jq are
   vendored, instead of tree-sitter-language-pack (7). Found while building
   stage 4.
7. The base set is 50 languages, not about 25: the binary grows from 54 to
   70 MiB (8 MiB gzipped), far under PyPI's limit, and a pre-commit tool
   finds the files a repository has without an extra to install (7).

## 16. Left to the builder

- The JSON encoding details not fixed above (key order, escaping).
- How the YAML path-to-position map is built (tree-sitter-yaml beside the
  serde document, as zizmor's `yamlpath`, or otherwise).
- The cache layout and lock file format beyond the fields named.
- Thread pool and walking crates (`ignore`, `rayon` are the obvious ones).
- How `zoll languages` and the grammar directory are laid out internally.
