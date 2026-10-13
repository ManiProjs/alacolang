" Detect Alaco source files.
augroup alaco_filetype
  autocmd!
  autocmd BufRead,BufNewFile *.aco setfiletype alaco
augroup END
