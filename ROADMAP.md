# Road map

## Implement features

- 'dfm diff' Add command 'diff'. When called without arguments must show files BothModified, TargetModified, SourceModified
file list. When called with path must show a diff (like git diff) using difftool, that must
be specified in config file, and have a default value "vimdiff {target} {source}".
The 'diff' command must not modify any files.
This feature must be added to README.md.

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
- The 'status --managed' does not show managed symlinks,
that are marked with 'LL' and are shown by 'status --all'

- In the README.md there is the example of unmanaged symlinks (?L),
but no example of managed symlinks (LL). Add the string with example to point 2.10 text field.

## Documents
- Switch license to GPL
- Add man page to package
- In README.md describe what is 'pull' command, that it works only local.

## Consider ideas
- Let pull take a path to where save the file

