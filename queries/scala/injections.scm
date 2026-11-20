; Copied by `make queries` from Helix's runtime/queries/scala/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; Matches these SQL interpolators:
;  - Doobie: 'sql', 'fr'
;  - Quill: 'sql', 'infix'
;  - Slick: 'sql', 'sqlu'
(interpolated_string_expression
  interpolator:
    ((identifier) @interpolator
     (#any-of? @interpolator "fr" "infix" "sql" "sqlu"))
  (interpolated_string) @injection.content
  (#set! injection.language "sql"))
