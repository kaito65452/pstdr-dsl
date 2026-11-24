" Vim syntax file
" Language: PackStudio DSL (.pstdr)
" Maintainer: pstdr-dsl contributors

if exists("b:current_syntax")
  finish
endif

" Comments
syntax match pstdrComment "//.*$" contains=@Spell
syntax region pstdrBlockComment start="/\*" end="\*/" contains=@Spell

" Directives
syntax match pstdrDirective "@addon\>"
syntax keyword pstdrDirective bind_target

" Declarations & Control Flow
syntax keyword pstdrKeyword const mut state func return
syntax keyword pstdrConditional if else
syntax keyword pstdrRepeat for while in
syntax keyword pstdrBoolean true false
syntax keyword pstdrConstant null

" Events, Timers & Actions (Wave 1 + Wave 2)
syntax keyword pstdrEvent on join right click hit break hurt die consume interact tick projectile spawn
syntax keyword pstdrEvent of with by type
syntax keyword pstdrTimer after every
syntax keyword pstdrAction cancel event damage ignite apply to send particle at
syntax keyword pstdrOperator and or not is within
syntax keyword pstdrPredicate sneaking sprinting swimming flying sleeping on_ground in_water holding

" Built-in globals
syntax keyword pstdrBuiltin world system console Math JSON

" Numbers, Durations & Distances
syntax match pstdrDuration "\<\d\+\(\.\d\+\)\?\(ms\|s\|m\|t\)\>"
syntax match pstdrDistance "\<\d\+\(\.\d\+\)\?b\>"
syntax match pstdrNumber "\<\d\+\(\.\d\+\)\?\>"

" Strings with {expr} interpolation
syntax region pstdrString start=+"+ skip=+\\\\\|\\"+ end=+"+ contains=pstdrInterpolation,pstdrEscape
syntax region pstdrInterpolation matchgroup=pstdrInterpDelim start=+{+ end=+}+ contained contains=TOP
syntax match pstdrEscape "\\." contained

" Embedded JavaScript inside `unsafe` blocks
syntax include @pstdrJS syntax/javascript.vim
unlet! b:current_syntax
syntax keyword pstdrUnsafeKeyword unsafe nextgroup=pstdrUnsafeName skipwhite
syntax match pstdrUnsafeName "\<[A-Za-z_][A-Za-z0-9_]*\>" contained nextgroup=pstdrUnsafeParams,pstdrUnsafeBlock skipwhite
syntax region pstdrUnsafeParams start="(" end=")" contained nextgroup=pstdrUnsafeBlock skipwhite
syntax region pstdrUnsafeBlock matchgroup=pstdrUnsafeBrace start="{" end="}" contained contains=@pstdrJS,pstdrUnsafeInnerBrace
syntax region pstdrUnsafeInnerBrace start="{" end="}" contained contains=@pstdrJS,pstdrUnsafeInnerBrace

" Highlight links
highlight default link pstdrComment Comment
highlight default link pstdrBlockComment Comment
highlight default link pstdrDirective PreProc
highlight default link pstdrKeyword Keyword
highlight default link pstdrConditional Conditional
highlight default link pstdrRepeat Repeat
highlight default link pstdrBoolean Boolean
highlight default link pstdrConstant Constant
highlight default link pstdrEvent Statement
highlight default link pstdrTimer Statement
highlight default link pstdrAction Function
highlight default link pstdrOperator Operator
highlight default link pstdrPredicate Type
highlight default link pstdrBuiltin Identifier
highlight default link pstdrDuration Number
highlight default link pstdrDistance Number
highlight default link pstdrNumber Number
highlight default link pstdrString String
highlight default link pstdrInterpDelim Special
highlight default link pstdrEscape SpecialChar
highlight default link pstdrUnsafeKeyword Special
highlight default link pstdrUnsafeName Function
highlight default link pstdrUnsafeBrace Delimiter

let b:current_syntax = "pstdr"
