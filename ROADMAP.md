# Road map

## Implement features

- 'dfm diff' Add command 'diff'. When called without arguments must show files BothModified, TargetModified, SourceModified
file list. When called with path must show a diff (like git diff) using difftool, that must
be specified in config file, and have a default value "vimdiff {target} {source}".
The 'diff' command must not modify any files.
This feature must be added to README.md.
The command of difftool must not be lauched in shell, it must be fork-exec.

- Add the following records to the ignore_file at `dfm init`
```text
\.rustup
\.cache
\.cargo
\.npm
\.state
\.local
```

- 'status' subcommand must take filter flag --encrypted (-e) to show encrypted files only.

- For synchronized files (managed, not ignored) 'status' command must show that file is encrypted, by (encrypted).

## Fix bugs
- In the README.md there is the example of unmanaged symlinks (?L),
but no example of managed symlinks (LL). Add the string with example to point 2.10 text field.

- (open) encrypted files are read fully into RAM (a few × file size).
  Consider streaming encryption (chunked AEAD) for very large secrets.

## Documents
- Switch license to GPL
- In README.md describe what is 'pull' and 'add' commands, that they work only locally.

