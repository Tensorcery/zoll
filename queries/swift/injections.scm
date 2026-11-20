; Copied by `make queries` from Helix's runtime/queries/swift/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; Upstream: https://github.com/alex-pinkus/tree-sitter-swift/blob/57c1c6d6ffa1c44b330182d41717e6fe37430704/queries/injections.scm

; Parse regex syntax within regex literals

((regex_literal) @injection.content
 (#set! injection.language "regex"))
