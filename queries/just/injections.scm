; Copied by `make queries` from Helix's runtime/queries/just/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; Specify nested languages that live within a `justfile`

; ================ Always applicable ================

; Highlight the RHS of `=~` as regex

; Highlight the value of attribute `[arg(pattern = "...")]` as regex

; ================ Global defaults ================

; Default recipe lines to be bash, but exclude interpolation nodes
; This prevents bash's rainbow brackets from interfering with just's {{ }} markers
; Use injection.combined to combine all recipe lines so bash can parse multi-line constructs
(recipe_body
  !shebang
  (recipe_line) @injection.content
  (#set! injection.language "bash")
  (#set! injection.combined))

; ================ Global language specified ================
; Global language is set with something like one of the following:
;
;    set shell := ["bash", "-c", ...]
;    set shell := ["pwsh.exe"]
;
; We can extract the first item of the array, but we can't extract the language
; name from the string with something like regex. So instead we special case
; two things: powershell, which is likely to come with a `.exe` attachment that
; we need to strip, and everything else which hopefully has no extension. We
; separate this with a `#match?`.
;
; Unfortunately, there also isn't a way to allow arbitrary nesting or
; alternatively set "global" capture variables. So we can set this for item-
; level external commands, but not for e.g. external commands within an
; expression without getting _really_ annoying. Should at least look fine since
; they default to bash. Limitations...
; See https://github.com/tree-sitter/tree-sitter/issues/880 for more on that.

; ================ Recipe language specified - Helix only ================

; Set highlighting for recipes that specify a language using builtin shebang matching
