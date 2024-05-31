# Dotfile Manager

## System Requirements
Linux kernel starting from 4.11 for the file creation time to be available.

## How Dotfile Managing is Performed

### Terminology
config file - the file in filesystem that contains parameters for a program.  
target file (TF) - a managed file in the target directory.  
source file (SF) - the backing up file in source directory.  
ctime - time when file was created. Set only when creation performed.
mtime - time when the last modification of a file was performed.  
atime - time then file was read the last time, is not used in the dotfile manager.  
exists - true/false, if file or directory present in filesystem.  
- Successful `add` subcommand modifies the creation and modification time of the source file to be equal to
the modification time of the target file and to each other.
- Successful `apply` subcommand modifies times the same way as `add` subcommand.

Regular run (non init):
1. try to read ~/.config/fdm/config.toml,
2. if not found try to read ~/.dfm.toml,
3. if not found error, print help.

### Init
Thi command modifies dotfile manager's config file if it exists.
Subcommand `init` is meant to run before any config files was created in target directory.
The `init` will check if `dot_prefix` config parameter is set to value used in source folder.
if config file exists, check that specified source dir exists and contains `.dfm-source` file,
if not create dir and file, print "already initialized"
exit
take a path, check if it != $HOME look for .fdm-source, if exists read the source dir path from it, if not exists
search in the path recursively until find a file .fdm-source, if not found
error, print help
create ~/.config/dfm/config.toml, if exists
do not override values, write source dir path to it, if source path has expanded $HOME prefix
write source path with a $HOME prepended and relative path.
Write $HOME to target path variable of the config
if --config is given then error, print help

### Add and Apply
Subcommand `add` copy files from target directory to source directory, subcommand `apply` copy from
source directory to target directory respectively.
Before perform any coping the check any modification conflict present.  
The check algorithm allows to figure out the fact that target file was edited by user or owning program, and the fact
that target file was edited by user or by `git`.
1. if !TF.exists && SF.exists then, `add` aborts with error, `apply` copies SF to TF.
2. if TF.exists && !TF.symlink && !SF.exists then `add` will copy TF to SF, `apply` will fail.
3. if TF.exists && TF.symlink && !SF.exists then `add` will fail and `apply` will fail.
4. if !TF.exists && !SF.exists then, `add` will fail, `apply` will fail.
5. if TF.exists && TF.symlink && SF.exists then, `add` do nothing and `apply` do nothing.
6. if TF.exists && !TF.symlink && SF.exists then, checks performed:
    1. if TF.mtime == SF.ctime && SF.ctime == SF.mtime then, no file was modified, `add` and `apply` will do nothing.
    2. if TF.mtime == SF.ctime && SF.ctime < SF.mtime, source file was modified, target file was not,
    `add` subcommand will overwrite changes in source file (conflict), `apply` subcommand will copy new version
    of source file to the target file (no conflict).
    3. if TF.mtime > SF.ctime && SF.ctime == SF.mtime then, target file was modified, source file was not,
    `add` subcommand will copy new version of the target file to the source file (no conflict), `apply` subcommand
    will overwrite new changes in the target file (conflict).
    4. if TF.mtime > SF.ctime && SF.ctime < SF.mtime then, both files was modified independently, both `add` and
    `apply` subcommands will overwrite new modifications (conflict).
