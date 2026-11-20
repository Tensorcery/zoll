; Copied by `make queries` from Helix's runtime/queries/go/injections.scm
; at 9de242bfac2aac9dfd0e4496eb25190e85c24e69, less what zoll does not evaluate
; (queries/README.md).
;
; This Source Code Form is subject to the terms of the Mozilla Public
; License, v. 2.0. If a copy of the MPL was not distributed with this
; file, You can obtain one at https://mozilla.org/MPL/2.0/.

; Inject markdown into documentation comments
;
; Go's comments are documentation comments when they are directly followed
; by one of Go's statements (e.g. `type`, `func`, `const`)
;
; This is only a partial implementation, which covers only
; block comments. For line comments (which are more common),
; upstream changes to the grammar are required.
(source_file
  (comment) @injection.content . (comment)* . [
    (package_clause) ; `package`
    (type_declaration) ; `type`
    (function_declaration) ; `func`
    (method_declaration) ; `func`
    (var_declaration) ; `var`
    (const_declaration) ; `const`
    ; var (
    ; 	A = 1
    ; 	B = 2
    ; )
    (var_spec)
    ; const (
    ; 	A = 1
    ; 	B = 2
    ; )
    (const_spec)
  ]
  (#set! injection.language "markdown"))

((comment) @injection.content
 (#match? @injection.content "^//go:generate")
 (#set! injection.language "bash"))

(call_expression
  (selector_expression) @_function
  (#any-of? @_function "regexp.Match" "regexp.MatchReader" "regexp.MatchString" "regexp.Compile" "regexp.CompilePOSIX" "regexp.MustCompile" "regexp.MustCompilePOSIX")
  (argument_list
    .
    [
      (raw_string_literal (raw_string_literal_content) @injection.content)
      (interpreted_string_literal (interpreted_string_literal_content) @injection.content)
    ]
    (#set! injection.language "regex")))

; https://pkg.go.dev/fmt#Printf
; https://pkg.go.dev/fmt#Sprintf
; https://pkg.go.dev/fmt#Scanf
; https://pkg.go.dev/fmt#Errorf

; https://pkg.go.dev/fmt#Fprintf
; https://pkg.go.dev/fmt#Fscanf
; https://pkg.go.dev/fmt#Sscanf

; https://pkg.go.dev/log#Printf
; https://pkg.go.dev/log#Fatalf
; https://pkg.go.dev/log#Panicf
; https://pkg.go.dev/log#Logger.Printf
; https://pkg.go.dev/log#Logger.Fatalf
; https://pkg.go.dev/log#Logger.Panicf
