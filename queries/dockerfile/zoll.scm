; zoll's own: Helix's queries look for `shell_fragment`, a node the grammar
; zoll has (tree-sitter-containerfile) does not make (queries/README.md).

; A `RUN` in shell form is bash, its line continuations left out.
((shell_command) @injection.content
 (#set! injection.language "bash"))

; So is the body of a `RUN <<EOF` heredoc.
((run_instruction
  (heredoc_block
    (heredoc_content) @injection.content))
 (#set! injection.language "bash"))
