# Road map

## Implement features

- Add the following records to the ignore_file at `dfm init`
```text
\.cache
\.cargo
\.npm
\.state
\.local
```
Calculate each path to ignore: if XDG_* path is a subpath of the target directory, then add it to ignore_file.
- 'status' subcommand must take filter flag --encrypted (-e) to show encrypted files only.
- For synchronized files (managed, not ignored) 'status' command must show that file is encrypted, by (encrypted).
- Automatically add files with encryption if they are located in directory path with 700 (or stricter) permissions of any of
the path component.
- subcommand 'status' must support combinations of short or long flags.
- add flag --editable (-e) for subcommand 'diff', with this flag the diff subcommand must allow to modify target file and source files.
- Exit with error if dfm is launched with root privileges (not necessary under root itself).
- Add 'sync' subcommand, that do 'add .' and 'pull .', does not work if conflicts detected, unless --force. But
even with --force the 'sync' must not modify conflicting files. If paths are provided 'sync' must 'add-pull' only on
there files/directories. The wolkdir-conflict-searching code must be shared between 'add', 'pull' and 'sync'.
The 'sync' must do nothing with unmanaged files, and must not return with error handing them. 'sync' must not 'add' unmanaged files.
'sync' must not 'pull' unpulled files.
- Add progress bar to action phase of command (now it works only in wallkdir phase).

## Fix bugs

- (open) encrypted files are read fully into RAM (a few × file size).
  Consider streaming encryption (chunked AEAD) for very large secrets.
- Do 'cd' to ignored subdir of target directory, and in target directory there is a modified file,
the 'dfm diff filename' will say that filename is ignored by the ignore regexp of the directory.
But must actually show the difference. That means that the program searches files appending there name to the current
directory path, not to the target directory path.
- 'dfm ignore ~/path', and path is present in target directory - ignore pattern unused, but must
must be used against the directory relevant to /, because the path expands in '/..'?
- 'dfm ignore dir' creates an ignore expression that matches a subdir of other dir in target dir.
But must be matched only to the target's subdir, the relevance of matching is broken.

## Documents

- Switch license to GPL.
- In README.md describe what is 'pull' and 'add' commands, that they work only locally.

## Considerations
- what if source file belongs to the user other than puller?

