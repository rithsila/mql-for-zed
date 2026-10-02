(
  (function_definition
    declarator: (function_declarator
      declarator: (identifier) @run)) @_mql-entry
  (#match? @run "^(OnInit|OnTick|OnStart|OnCalculate)$")
  (#set! tag mql5)
)
