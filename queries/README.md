# queries

Where a language holds code of another: one directory per host language,
`queries/<language>/injections.scm`, in tree-sitter's query language - the
same files editors use to highlight awk inside a shell string. A query says
where code of another language sits; it judges nothing. What is allowed there
is a policy's call, outside this repository.

## Where the files come from

`injections.scm` is copied, not written, by `make queries` (the
`sync_queries` example of `zoll_core`) at the Helix commit it pins
(docs/design.md, sections 6 and 14):

1. Helix, `runtime/queries/<language>/injections.scm` (MPL-2.0), with the
   files it `; inherits:` read in - first, since zoll implements its
   injection semantics;
2. nvim-treesitter (Apache-2.0) - only for a language Helix has no file for.
   None of the base set needs it today, so the script does not fetch it yet.

One source per language, never merged. The script strips what zoll does not
evaluate (editor-only directives such as Neovim's `#offset!`) and leaves out
an injection that:

- names a language zoll does not have: `comment`, `jsdoc`, Lua patterns,
  format strings, `markdown.inline`;
- uses a predicate zoll does not evaluate (zoll evaluates the `#eq?`,
  `#match?` and `#any-of?` families and `#set!`);
- does not compile against the grammar zoll is built with: most of
  svelte's, some of kotlin's, just's and nginx's, all of dockerfile's.

It prints each with its reason. A language left with no injection has no
file. Updating is raising the pinned commit and running the script again;
the diff is reviewed like any other.

`languages.toml` is copied from Helix's `languages.toml` by the same script:
each language and the regex an injection's language name (a Markdown
fence's info string) is matched with, whole.

`<language>/zoll.scm` is zoll's own, read after `injections.scm`, for what
Helix's queries miss with zoll's grammar: `dockerfile/zoll.scm` finds a
`RUN`'s shell, since tree-sitter-containerfile has no `shell_fragment`. It is
MIT, like the rest of the repository; the script never touches it.

Each copied file carries the MPL's notice and stays a file of its own; a
part nvim-treesitter wrote inside a Helix file keeps its attribution. The
license texts are `LICENSE-MPL-2.0` and `LICENSE-Apache-2.0` here. jscpd
skips this directory: the sources repeat each other by design.
