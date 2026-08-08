# Road map

## Implement features

- 'dfm status filepath' For dfm status add possibility to take several paths.
If several paths provided by user then 'status' command must show
statuses only of these files/directories the same way it shows statuses for ALL files/directories.
If flags provided along the paths then status must take these flags in to account.
Update README.md: add descriptions of this feature in to paragraph of status command.

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

## Fix bugs
- The 'status --managed' does not show managed symlinks,
that are marked with 'LL' and are shown by 'status --all'

- In the README.md there is the example of unmanaged symlinks (?L),
but no example of managed symlinks (LL). Add the string with example to point 2.10 text field.

## Documents
- Switch license to GPL
- add man page to package
- Describe what is 'pull' command, that it works only local.

## Consider ideas
- let pull take a path to where save the file

