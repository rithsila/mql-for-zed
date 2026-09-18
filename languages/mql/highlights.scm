; =============================================================================
; MQL4/MQL5 Syntax Highlighting (uses Zed built-in cpp grammar)
; =============================================================================

; ── MQL Variable Modifiers ───────────────────────────────────────────────────
((identifier) @keyword.modifier
 (#any-of? @keyword.modifier "input" "sinput" "extern"))

; ── MQL Built-in Types ───────────────────────────────────────────────────────
((type_identifier) @type.builtin
 (#any-of? @type.builtin "datetime" "color" "uchar" "ushort" "uint" "ulong"))

; ── Primitive Types ──────────────────────────────────────────────────────────
(primitive_type) @type.builtin

; ── MQL Event Handlers ───────────────────────────────────────────────────────
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function.builtin)
 (#any-of? @function.builtin
   "OnInit" "OnDeinit" "OnTick" "OnTimer" "OnStart" "OnStop"
   "OnCalculate" "OnTrade" "OnTradeTransaction"
   "OnChartEvent" "OnBookEvent" "OnTesterInit"
   "OnTesterDeinit" "OnTesterPass" "OnTester" "OnNewBar"))

; ── User-defined Functions ───────────────────────────────────────────────────
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @function))

; ── Function Calls ───────────────────────────────────────────────────────────
(call_expression
  function: (identifier) @function.call)

(call_expression
  function: (field_expression
    field: (field_identifier) @function.method.call))

; ── Type Definitions ─────────────────────────────────────────────────────────
(type_identifier) @type

(struct_specifier   name: (type_identifier) @type.definition)
(class_specifier    name: (type_identifier) @type.definition)
(enum_specifier     name: (type_identifier) @type.definition)
(union_specifier    name: (type_identifier) @type.definition)

; ── Control Flow ─────────────────────────────────────────────────────────────
[
  "if" "else" "while" "for" "do"
  "switch" "case" "default"
  "break" "continue" "return"
  "goto" "try" "catch" "throw"
] @keyword.control

; ── Declaration Keywords ─────────────────────────────────────────────────────
[
  "class" "struct" "enum" "union"
  "namespace" "template" "typedef" "typename"
] @keyword.type

; ── Modifier Keywords ────────────────────────────────────────────────────────
[
  "const" "volatile" "static" "inline"
  "register" "explicit" "virtual"
  "override" "final" "mutable"
  "public" "private" "protected"
] @keyword.modifier

; ── Other Keywords ───────────────────────────────────────────────────────────
["new" "delete" "sizeof"] @keyword

; ── Operators ────────────────────────────────────────────────────────────────
[
  "=" "+=" "-=" "*=" "/=" "%=" "&=" "|=" "^=" "<<=" ">>="
  "==" "!=" "<" ">" "<=" ">="
  "+" "-" "*" "/" "%"
  "&&" "||" "!"
  "&" "|" "^" "~" "<<" ">>"
  "++" "--"
  "->" "::" "?" ":"
] @operator

; ── String Literals ──────────────────────────────────────────────────────────
(string_literal)    @string
(system_lib_string) @string
(char_literal)      @character
(escape_sequence)   @character.escape

; ── Number Literals ──────────────────────────────────────────────────────────
(number_literal) @number

; ── ALL_CAPS constants (MQL enums / defines) ─────────────────────────────────
((identifier) @constant
 (#match? @constant "^[A-Z][A-Z0-9_]{2,}$"))

; ── Comments ─────────────────────────────────────────────────────────────────
(comment) @comment

; ── Preprocessor ─────────────────────────────────────────────────────────────
(preproc_directive)  @keyword.directive
(preproc_include)    @keyword.import
(preproc_def name:   (identifier) @constant)

(preproc_include path: (string_literal)    @string.special.path)
(preproc_include path: (system_lib_string) @string.special.path)

; ── Parameters ───────────────────────────────────────────────────────────────
(parameter_declaration declarator: (identifier) @variable.parameter)

; ── Member Access ────────────────────────────────────────────────────────────
(field_identifier) @variable.member

; ── Namespace ────────────────────────────────────────────────────────────────
(namespace_identifier) @namespace

; ── Punctuation ──────────────────────────────────────────────────────────────
[";" ","] @punctuation.delimiter
["(" ")" "[" "]" "{" "}"] @punctuation.bracket
