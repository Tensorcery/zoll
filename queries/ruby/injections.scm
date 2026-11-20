; Copied by `make queries` from Helix's runtime/queries/ruby/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

((heredoc_body
  (heredoc_content) @injection.content
  (heredoc_end) @name
  (#set! injection.language "sql"))
 (#eq? @name "SQL"))

((heredoc_body
  (heredoc_content) @injection.content
  (heredoc_end) @name
  (#set! injection.language "graphql"))
 (#any-of? @name
       "GQL"
       "GRAPHQL"))

; `<command>`
; %x{<command>}
(subshell
  (string_content) @injection.content
  (#set! injection.language "bash"))

(call
  method: (identifier) @_method (#any-of? @_method "system" "spawn" "exec")
  arguments: (argument_list
    (string
      (string_content) @injection.content))
  (#set! injection.language "bash"))
