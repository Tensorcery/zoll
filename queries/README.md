# queries

Where a language holds code of another: one directory per host language,
`queries/<language>/injections.scm`, in tree-sitter's query language - the
same files editors use to highlight awk inside a shell string. A query says
where code of another language sits; it judges nothing. What is allowed there
is a policy's call, outside this repository.

## Where the files come from

They are copied, not written, by a sync script at a pinned commit of each
source (docs/design.md, sections 6 and 14):

1. Helix, `runtime/queries/<language>/injections.scm` (MPL-2.0) - first,
   since zoll implements its injection semantics;
2. nvim-treesitter (Apache-2.0) - only for a language Helix has no file for.

One source per language, never merged. The script strips what zoll does not
evaluate (editor-only directives such as Neovim's `#offset!`), drops an
injection whose predicate zoll cannot evaluate and lists it. Updating is
raising the pinned commit and running the script again; the diff is
reviewed like any other.

Each copied file keeps its license header and stays a file of its own; a
part nvim-treesitter wrote inside a Helix file keeps its attribution. A file
zoll writes itself - for a gap such as SQL in Python strings - is MIT, like
the rest of the repository. jscpd skips this directory: the sources repeat
each other by design.
