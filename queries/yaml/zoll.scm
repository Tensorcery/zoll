; zoll's own injections for YAML, in place of Helix's (queries/README.md):
; the shell of CI and container files, in every spelling YAML has for it.
; Each scalar is decoded as YAML reads it (crates/zoll_core/src/decode.rs).
; A key's text is matched with its quotes: `run`, `"run"`, `'run'`.

; `run:` of GitHub Actions, `command:` and `commands:` of Buildkite and
; others: a scalar, in a block or a flow mapping.
(_
  key: (flow_node) @_key
  value: (flow_node
    [(plain_scalar) (double_quote_scalar) (single_quote_scalar)] @injection.content)
  (#match? @_key "^[\"']?(run|command|commands)[\"']?$")
  (#set! injection.language "bash")
  (#set! injection.include-children))

(block_mapping_pair
  key: (flow_node) @_key
  value: (block_node (block_scalar) @injection.content)
  (#match? @_key "^[\"']?(run|command|commands)[\"']?$")
  (#set! injection.language "bash")
  (#set! injection.include-children))

; A list of commands, each a script; not an argv list, which holds `-c`.
(block_mapping_pair
  key: (flow_node) @_key
  value: (block_node
    (block_sequence
      (block_sequence_item
        [(flow_node
           [(plain_scalar) (double_quote_scalar) (single_quote_scalar)] @injection.content)
         (block_node (block_scalar) @injection.content)]))) @_list
  (#match? @_key "^[\"']?(run|command|commands)[\"']?$")
  (#not-match? @_list "(^|\n)[ \t]*-[ \t]+[\"']?-c[\"']?[ \t]*(\n|$)")
  (#set! injection.language "bash")
  (#set! injection.include-children))

(_
  key: (flow_node) @_key
  value: (flow_node
    (flow_sequence
      (flow_node
        [(plain_scalar) (double_quote_scalar) (single_quote_scalar)] @injection.content))) @_list
  (#match? @_key "^[\"']?(run|command|commands)[\"']?$")
  (#not-match? @_list "[\\[,][ \t\n]*[\"']?-c[\"']?[ \t\n]*[,\\]]")
  (#set! injection.language "bash")
  (#set! injection.include-children))

; An argv list (Kubernetes, Compose): the item after `-c` is the script;
; `sh` and `-c` are not.
(block_sequence
  (block_sequence_item (flow_node) @_flag)
  .
  (block_sequence_item
    [(flow_node
       [(plain_scalar) (double_quote_scalar) (single_quote_scalar)] @injection.content)
     (block_node (block_scalar) @injection.content)])
  (#match? @_flag "^[\"']?-c[\"']?$")
  (#set! injection.language "bash")
  (#set! injection.include-children))

(flow_sequence
  (flow_node) @_flag
  .
  (flow_node
    [(plain_scalar) (double_quote_scalar) (single_quote_scalar)] @injection.content)
  (#match? @_flag "^[\"']?-c[\"']?$")
  (#set! injection.language "bash")
  (#set! injection.include-children))
