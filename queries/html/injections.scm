; Copied by `make queries` from Helix's runtime/queries/html/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

((script_element
  (raw_text) @injection.content)
 (#set! injection.language "javascript"))

((style_element
  (raw_text) @injection.content)
 (#set! injection.language "css"))

; e.g. `<input pattern="[Bb]anana|[Cc]herry" />
(attribute
  (attribute_name) @_attr (#eq? @_attr "pattern")
  (quoted_attribute_value
    (attribute_value) @injection.content)
  (#set! injection.language "regex"))
