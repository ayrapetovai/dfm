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
- Command 'dfm status filepath' must print "filepath is up-to-date." instead of "All up-to-date.".
- Add flag --encrypted (-e) for subcommand 'status', when givent dfm must print a block of filenames relative to target path, like other commands
that are encrypted in source directory.

## Documents

- Switch license to GPL.
- In README.md describe what is 'pull' and 'add' commands, that they work only locally.
- Shrink README.md and context.txt

## Considerations
- what if source file belongs to the user other than puller?

