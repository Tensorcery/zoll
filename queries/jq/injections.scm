; Copied by `make queries` from Helix's runtime/queries/jq/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

;; From nvim-treesitter, contributed by @ObserverOfTime et al.

; test(val)
(query
  ((funcname) @_function
    (#any-of? @_function "test" "match" "capture" "scan" "split" "splits" "sub" "gsub"))
  (args
    .
    (query
      (string) @injection.content
      (#set! injection.language "regex"))))

; test(regex; flags)
(query
  ((funcname) @_function
    (#any-of? @_function "test" "match" "capture" "scan" "split" "splits" "sub" "gsub"))
  (args
    .
    (args
      (query
        (string) @injection.content
        (#set! injection.language "regex")))))
