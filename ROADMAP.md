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

- In the README.md there is the example of unmanaged symlinks (?L),
but no example of managed symlinks (LL). Add the string with example to point 2.10 text field.
- (open) encrypted files are read fully into RAM (a few × file size).
  Consider streaming encryption (chunked AEAD) for very large secrets.
- Command 'dfm diff filepath', where filepath does not exists prints "{filepath} is not managed",
but this info must be displayed is there is a source file for a given path. When the filepath does
not exists in target directory and corresponding source file does not exist than message "{filepath} does not exists"
must be displayed.
- subcommand status --modified shows unused ignore patterns with block header.
--unpulled also shows unused ignore patterns with block header. And --managed...
- subcommand status --unused-patterns does not output the block header "Unused ignore patterns:"
- 'dfm add .config/dfm' does not add the dfm's configuration file to source directory.
- Reproduce: add README.md in .dfm_ignore_source in source directory, if README.md in target directory is managed and removed,
the 'dfm status' will say that it is not pulled, but must ignore it.

## Documents

- Switch license to GPL.
- In README.md describe what is 'pull' and 'add' commands, that they work only locally.

## Considerations
- what if source file belongs to the user other than puller?

