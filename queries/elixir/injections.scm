; Copied by `make queries` from Helix's runtime/queries/elixir/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; Elixir Code Comments

; Elixir Regular Expressions
((sigil
  (sigil_name) @_sigil_name
  (quoted_content) @injection.content)
 (#match? @_sigil_name "^(R|r)$")
 (#set! injection.language "regex")
 (#set! injection.combined))

; Elixir Markdown Documentation
(unary_operator
  operator: "@"
  operand: (call
  target: ((identifier) @_identifier (#match? @_identifier "^(module|type|short)?doc$"))
    (arguments [
      (string (quoted_content) @injection.content)
      (sigil (quoted_content) @injection.content)
  ])) (#set! injection.language "markdown"))

; Zigler Sigils
((sigil
  (sigil_name) @_sigil_name
  (quoted_content) @injection.content)
 (#match? @_sigil_name "^(Z|z)$")
 (#set! injection.language "zig")
 (#set! injection.combined))

; Jason Sigils
((sigil
  (sigil_name) @_sigil_name
  (quoted_content) @injection.content)
 (#match? @_sigil_name "^(J|j)$")
 (#set! injection.language "json")
 (#set! injection.combined))

; Phoenix Live View HEEx Sigils

; Phoenix Live View Component Macros
(call
  (identifier) @_identifier
  (arguments
    (atom)+
    (keywords (pair
      ((keyword) @_keyword (#eq? @_keyword "doc: "))
      [
        (string (quoted_content) @injection.content)
        (sigil (quoted_content) @injection.content)
      ]))
  (#match? @_identifier "^(attr|slot)$")
  (#set! injection.language "markdown")))
