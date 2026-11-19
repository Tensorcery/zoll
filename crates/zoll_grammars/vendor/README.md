# Vendored grammars

The generated parsers of the grammars that have no crate on crates.io zoll
can use - none under a permissive license, or one that builds only with an
older tree-sitter - copied as they are. Each directory keeps its license.

| Language | Source | Revision | License |
|---|---|---|---|
| awk | https://github.com/Beaglefoot/tree-sitter-awk | `34bbdc7cce8e803096f47b625979e34c1be38127` | MIT |
| gotmpl | https://github.com/ngalaiko/tree-sitter-go-template | `06b2fd2ac32a7be71cdcf925eeeb868a68e2acde` | MIT |
| jq | https://github.com/flurie/tree-sitter-jq | `c204e36d2c3c6fce1f57950b12cabcc24e5cc4d9` | BSD-3-Clause |
| just | https://github.com/IndianBoy42/tree-sitter-just | `5685543a6e64f66335e25518c9ae8ffa1dae3d01` | Apache-2.0 |
| perl | https://github.com/tree-sitter-perl/tree-sitter-perl, branch `release` | `19422548269fb18266a63662704caf2307ecf854` | MIT |
| scss | https://github.com/tree-sitter-grammars/tree-sitter-scss | `5da5ba71a558b007b352c505bfcd9095ae337022` | MIT |

Copied: `src/parser.c`, `src/scanner.c` and the headers it includes when
there is one, `src/node-types.json` and `src/tree_sitter/`. gotmpl is the
repository's base grammar, not its `dialects/helm`.

Why each is here:

- awk: its only crate is a fork published by someone other than the
  grammar's author;
- gotmpl: no crate;
- jq: the crate `tree-sitter-jq` is GPL-3.0-or-later;
- just, perl: their crates need an older tree-sitter than zoll's;
- scss: its crate (1.0.0, 2024) binds tree-sitter itself, at any version,
  not through `tree-sitter-language`.

just's `package.json` says MIT; its `LICENSE` is Apache-2.0, which this
table follows.

To update one, copy the same files from a newer revision and change the
revision here.
