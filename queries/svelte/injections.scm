; Copied by `make queries` from Helix's runtime/queries/svelte/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; <script> content defaults to JavaScript

; <style> content defaults to CSS

; <script> or <style> with "lang" attribute

; Inline style attribute
((attribute
  (attribute_name) @_style_name
  (quoted_attribute_value (attribute_value) @injection.content))
  (#eq? @_style_name "style")
  (#set! injection.language "css"))

; Typed expression content

; Shorthand attributes ({foo})

; Tag expressions ({@const}, {@render}, {@html}, {@debug}, {@attach}, {:else if})

; Else-if clause

; If block expressions

; Each block expressions and bindings

; Await block expressions and bindings

; Key block expressions

; Snippet parameters and type parameters

; Comments
