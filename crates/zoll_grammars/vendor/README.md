# Vendored grammars

The generated parsers of the grammars that have no crate on crates.io under
a permissive license, copied as they are. Each directory keeps its license.

| Language | Source | Revision | License |
|---|---|---|---|
| awk | https://github.com/Beaglefoot/tree-sitter-awk | `34bbdc7cce8e803096f47b625979e34c1be38127` | MIT |
| jq | https://github.com/flurie/tree-sitter-jq | `c204e36d2c3c6fce1f57950b12cabcc24e5cc4d9` | BSD-3-Clause |

Copied: `src/parser.c`, `src/scanner.c` when there is one,
`src/node-types.json` and `src/tree_sitter/`. The crate `tree-sitter-jq` on
crates.io is GPL-3.0-or-later; awk's only crate there is a fork published by
someone other than the grammar's author.

To update one, copy the same files from a newer revision and change the
revision here.
