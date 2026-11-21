; zoll's own: in the just grammar zoll vendors a recipe line holds its text
; as a child, which Helix's pattern leaves out, so nothing of the line is
; left (queries/README.md). The line whole, interpolations and all.
(recipe_body
  !shebang
  (recipe_line) @injection.content
  (#set! injection.language "bash")
  (#set! injection.combined)
  (#set! injection.include-children))
