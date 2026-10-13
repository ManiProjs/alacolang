if exists('b:current_syntax')
  finish
endif

syn case match
syn keyword alacoKeyword fn struct let mut if else while for in return match import as
syn keyword alacoBoolean true false
syn keyword alacoType Int Float Bool String Void
syn keyword alacoBuiltin print println
syn match alacoNumber '\<\d\+\(\.\d\+\)\?\>'
syn match alacoNumber '\<0[xX][0-9a-fA-F]\+\>'
syn match alacoOperator '[-+*/%=!<>|&]\{1,2}'
syn match alacoDelimiter '[{}()\[\],.;:]'
syn region alacoString start='"' skip='\\.' end='"' contains=alacoEscape
syn match alacoEscape '\\[\\"nrt0]' contained
syn match alacoComment '//.*$' contains=alacoTodo
syn keyword alacoTodo TODO FIXME NOTE XXX contained
syn region alacoBlockComment start='/\*' end='\*/' contains=alacoTodo

hi def link alacoKeyword Keyword
hi def link alacoBoolean Boolean
hi def link alacoType Type
hi def link alacoBuiltin Function
hi def link alacoNumber Number
hi def link alacoOperator Operator
hi def link alacoDelimiter Delimiter
hi def link alacoString String
hi def link alacoEscape SpecialChar
hi def link alacoComment Comment
hi def link alacoBlockComment Comment
hi def link alacoTodo Todo

let b:current_syntax = 'alaco'
