; Copied by `make queries` from Helix's runtime/queries/cpp/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

((preproc_arg) @injection.content
 (#set! injection.language "c")
 (#set! injection.include-children))

((preproc_arg) @injection.content
 (#set! injection.language "cpp")
 (#set! injection.include-children))

(raw_string_literal
  delimiter: (raw_string_delimiter) @injection.language
  (raw_string_content) @injection.content)
