; Top-level function definitions appear in the outline
(function_definition
  declarator: (function_declarator
    declarator: (identifier) @name)) @item

; Class definitions
(class_specifier
  name: (type_identifier) @name) @item

; Struct definitions
(struct_specifier
  name: (type_identifier) @name) @item

; Enum definitions
(enum_specifier
  name: (type_identifier) @name) @item

; Typedef
(type_definition
  declarator: (type_identifier) @name) @item
