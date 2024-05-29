# Dotfile Manager

## Algorithm of Detecting Conflicts in Managed Files
### Protocol of Time Modification for Managed Files
target file (TF) - a managed file in the target directory.  
source file (SF) - the backing up file in source directory.  
ctime - time of creation of a file.  
mtime - time of the last modification of a file.  
- Successful `add` subcommand modifies the creation and modification time of the source file to be equal to
the modification time of the target file and to each other.
- Successful `apply` subcommand modifies times the same way as `add` subcommand.

### Checks Made by Add and Apply Subcommands
The algorithm allows to figure out the fact that target file was edited by user or owning program, and the fact
that target file was edited by user or by `git`.
1. if TF.mtime == SF.ctime && SF.ctime == SF.mtime then, no file was modified, `add` and `apply` will do nothing.
2. if TF.mtime == SF.ctime && SF.ctime < SF.mtime, source file was modified, target file was not,
`add` subcommand will overwrite changes in source file (conflict), `apply` subcommand will copy new version
of source file to the target file (no conflict).
3. if TF.mtime > SF.ctime && SF.ctime == SF.mtime then, target file was modified, source file was not,
`add` subcommand will copy new version of the target file to the source file (no conflict), `apply` subcommand
will override new changes in the target file (conflict).
4. if TF.mtime > SF.ctime && SF.ctime < SF.mtime then, both files was modified independently, both `add` and
`apply` subcommands will overwrite new modifications (conflict).
