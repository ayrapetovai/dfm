# Dotfile Manager
This program is designed to maintain copies of configuration files from the home directory
using a separate directory under a version control system.

## System Requirements
Linux kernel starting from 4.11 for the file creation time to be available.  
Filesystem one of: ufs2, zfs, ext4, btrfs, jfs.

## How Dotfile Managing is Performed

### Terminology
config file - the file in filesystem that contains parameters for the program.  
target directory - is set to be root for all files/directories to managed by dfm. There
can be only target directory for the whole filesystem.
source directory - used to store the information and copies of the managed objects.  
target file (TF) - a managed file in the target directory.  
source file (SF) - the backing up file in source directory.  
managed file/directory/symlink - the filesystem object for which will be created a corresponding object
in the source directory.
btime - time when file was created. Set only when creation performed.
mtime - time when the last modification of a file was performed.  
btime - time then file was read the last time, is not used in the dotfile manager.  
exists - true/false, if file or directory present in filesystem.  
- Successful `add` subcommand modifies the creation and modification time of the source file to be equal to
the modification time of the target file and to each other.
- Successful `apply` subcommand modifies times the same way as `add` subcommand.

Regular run (non init subcommand):
1. try to read ~/.config/dfm/config.toml,
2. if not found then try to read ~/.dfm.toml,
3. if not found then raise an error and print help.

## Ignored Files
The source directory is ignored by default, yet the directory that contains it is not ignored.
All files and subdirectories of the containing source directory can be added under the management.

### Init
Thi command modifies dotfile manager's config file if it exists.
Subcommand `init` is meant to run before any config files was created in target directory.
The `init` will check if `dot_prefix` config parameter is set to value used in source folder.
if config file exists, check that specified source dir exists and contains `.dfm-source` file,
if not create dir and file, print "already initialized" and exit
take a path, check if it != $HOME look for .fdm-source, if exists read the source dir path from it, if not exists
search in the path recursively until find a file .fdm-source, if not found then error and print help
create ~/.config/dfm/config.toml, if exists
do not override values, write source dir path to it, if source path has expanded $HOME prefix
write source path with a $HOME prepended and relative path.
Write $HOME to target path variable of the config
if --config is given then error, print help

### Add and Apply Conflict Detection
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

### Add
The subcommand take the paths of the target directory (does not operate on paths in the source directory, unlike
the other commands) and creates corresponding files in the source directory.
Subcommand traverses in depth all given path to locate the files, each file can be:
- a symlink, that points not into the source directory
    - if --overwrite then create a symlink file in the source directory,
    - otherwise do nothing
- a symlink, that points into the source directory pointing at the corresponding file
    - do nothing
- a symlink, that points into the source directory pointing at the non-corresponding file
    - if --overwrite then create a symlink file in source directory
- a symlink, that has an associated symlink file in the source directory
    - check if the symlink and the symlink file are pointing to the same file,
    if not, update the symlink file to point to the same file as tye symlink.
- a symlink, that has no associated symlink file in the source directory
    - if --overwrite then create a symlink file.
- an existing file, that has no corresponding file in the source directory
    - create a corresponding file.
- an existing file, that has a corresponding file in the source directory
    - if changes in target and source files are not conflicting then
    copy target file to the source file
- a non-existing file, that has no corresponding file in the source directory
    - error "file not found"
- a non-existing file, that has a corresponding file in the source directory
    - error "file not found"

### Apply
The subcommand takes the names of a files from target directory.
If the specified filename does not exist in target directory, then `apply` will calculate the corresponding names
in the source directory. If there is no such a file in source directory - error.
For existing target files: replacement, for non-existing files: creation (does not require special conditions).
Replacement checks if there is no conflict.
Traverse all directories in given paths, get the list of files to work on
each file in the target directory could be:
- a symlink, that points not into the source directory
    - exit with error, or remove if --overwrite?
- a symlink, that points into the source directory pointing at the corresponding file
    - do nothing
- a symlink, that points into the source directory pointing at the non-corresponding file
    - exit with error, if --overwrite then remove the link and create one pointing to the right file
- a symlink, that has an associated symlink file in the source directory
    - if the link points to the file specified in the source symlink file then do nothing
    - otherwise error or if --overwrite then recreate  the link.
- an existing file, that has no corresponding file in the source directory
    - error "target file is not managed"
- an existing file, that has a corresponding file in the source directory
    - if target file was not modified then overwrite it with the source file,
    - otherwise error or if --overwrite then overwrite or if --merge call merge tool
- a non-existing file, that has no corresponding file in the source directory
    - error "file not found and not managed"
- a non-existing file, that has a corresponding file in the source directory
    - copy the source file to the path of a target file

The `apply` subcommand is able to take a path from the source directory,
to make is easier to copy just cloned files, that don't yet exist in the home directory.
Each file in the source directory could be:
- an existing file, that has no corresponding file in the target directory
    - copy file from source directory to the path of the target file
- an existing file, that has a corresponding file in the target directory
    - if target file is not modified then copy source file to the path of the target file
    - or error or if --overwrite then copy, or is --merge then run merge
- an existing file, that has a corresponding symlink in the target directory
    - do nothing, but if the symlink pints to the wrong file recreate it if --overwrite
- a non-existing file
    - error "file does not exist and is not managed"

### Forget
The `forget` subcommand is for removing files from the source directory. It can take
path to either a path to a file in target directory or a path to a file in the source directory.
Traverse all directories in given paths, get the list of files to work on
each file in the target directory could be:
- a symlink, that points not into the source directory
    - do nothing
- a symlink, that points into the source directory pointing at the corresponding file
    - remove corresponding file and the symlink
- a symlink, that points into the source directory pointing at the non-corresponding file
    - remove the symlink only
- a symlink, has an associated symlink file in the source directory
    - if symlink pointee corresponds to the symlink file then remove symlink file
    - otherwise ask for the --overwrite flag
- a symlink, has no associated symlink file in the source directory
    - do nothing
- an existing file, that has no corresponding file in the source directory
    - do nothing
- an existing file, that has a corresponding file in the source directory
    - if the corresponding source file was not modified then remove it
    - otherwise ask for the --overwrite flag
- a non-existing file, that has no corresponding file in the source directory
    - do nothing
- a non-existing file, that has a corresponding file in the source directory
    - if corresponding file was not modified then remove it
    - otherwise ask for the flag --overwrite

The `forget` subcommand is able to take a path from the source directory,
to make is easier to remove files.
Each file in the source directory could be:
- an existing file, that has no corresponding file in the target directory
    - if source file ws not modified then remove it
    - otherwise ask for the flag --overwrite
- an existing file, that has a corresponding file in the target directory
    - if the source file was not modified or both files was not modified then
    remove source file.
    - or ask for the flag --overwrite to remove the source file.
- an existing file, that has a corresponding symlink in the target directory
    - check if symlink points to the same pointee as the source symlink file then
    remove the source symlink file
    - otherwise ask for the flag --overwrite to remove it
- a non-existing file
    - error "file does not exist"
