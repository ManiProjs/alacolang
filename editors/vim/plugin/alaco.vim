if exists('g:loaded_alaco_plugin')
  finish
endif
let g:loaded_alaco_plugin = 1

function! s:AlacoRunCommand(action) abort
  if &filetype !=# 'alaco' && expand('%:e') !=# 'aco'
    echoerr 'Alaco commands require an open .aco file'
    return
  endif
  if empty(expand('%:p'))
    echoerr 'Save the Alaco file before running a command'
    return
  endif
  execute '!' . shellescape(get(g:, 'alaco_executable', 'alaco')) . ' ' . a:action . ' ' . shellescape(expand('%:p'))
endfunction

command! AlacoCheck call <SID>AlacoRunCommand('check')
command! AlacoBuild call <SID>AlacoRunCommand('build')
command! AlacoRun call <SID>AlacoRunCommand('run')
