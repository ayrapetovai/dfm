# Dotfile Manager
This program is designed to maintain copies of configuration files from the home directory
using a separate directory under a version control system.
- safe copy from target to source and vise versa
- show all kinds of statuses
- ignore specified files in target and source directories
- call mergetool on conflicts (two-way merge)
- backup files before overwriting
- tracking symlinks and files they point to, if in source directory
- encrypt files and directories with AES
- process hooks on any stage of a subcommand

## How Dotfile Managing is Performed

### Terminology
config file - the file in filesystem that contains parameters for the program.  
state file - file in which the synchronization time, target and source directory paths are stored.  
target directory - is set to be root for all files/directories to managed by dfm. There
can be only target directory for the whole filesystem.
source directory - used to store the information and copies of the managed objects.  
target file (TF) - a managed file in the target directory.  
source file (SF) - the backing up file in source directory.  
managed file/directory/symlink - the filesystem object for which will be created a corresponding object
in the source directory.
mtime - time when the last modification of a file was performed.  
btime - time then file was read the last time, is not used in the dotfile manager.  
exists - true/false, if file or directory present in filesystem.  
- Successful `add` subcommand modifies the synchronization time in state file and modification time of the source file to be equal to
the modification time of the target file and to each other.
- Successful `pull` subcommand modifies times the same way as `add` subcommand.

Regular run (non init subcommand):
1. try to read `~/.config/dfm/config.toml`,
2. if not found then try to read `~/.dfm.toml`,
3. if not found then raise an error and print help.

### Encryption
While doing `init`, `add` or `pull` dfm use config specified command to obtain a passphrase.
The passphrase is used to encrypt/decrypt files.
For this config file must have set a property - cli command that will provide the passphrase.
That is why the `init` must pull the config file from the source directory to have the passphrase being ready for
decryption and encryption. By default, the command is `read -s; echo $REPLY` (no variable expansion).
The command is run by `$SHELL -c '{}'` (no variable expansion), this also must be configurable.
Subcommands warns if sensible files are added without encryption: .ssh, ...

### Backups
Target and source files can be copied to the `~/.local/state/dfm/**` and gzipped before being
overwritten by commands `add`, `pull` or `merge`.

### Ignored Files
The source directory is ignored by default, yet the directory that contains it is not ignored.
All files and subdirectories of the containing source directory can be added under the management.

### Add and Pull Conflict Detection
Subcommand `add` copy files from target directory to source directory, subcommand `pull` copy from
source directory to target directory respectively.
Before perform any coping the check any modification conflict present.  
The check algorithm allows to figure out the fact that target file was edited by user or owning program, and the fact
that target file was edited by user or by `git`.
1. if !TF.exists && SF.exists then, `add` aborts with error, `pull` copies SF to TF.
2. if TF.exists && !TF.symlink && !SF.exists then `add` will copy TF to SF, `pull` will fail.
3. if TF.exists && TF.symlink && !SF.exists then `add` will fail and `pull` will fail.
4. if !TF.exists && !SF.exists then, `add` will fail, `pull` will fail.
5. if TF.exists && TF.symlink && SF.exists then, `add` do nothing and `pull` do nothing.
6. if TF.exists && !TF.symlink && SF.exists then, checks performed:
    1. if TF.mtime == SF.ctime && SF.ctime == SF.mtime then, no file was modified, `add` and `pull` will do nothing.
    2. if TF.mtime == SF.ctime && SF.ctime < SF.mtime, source file was modified, target file was not,
    `add` subcommand will overwrite changes in source file (conflict), `pull` subcommand will copy new version
    of source file to the target file (no conflict).
    3. if TF.mtime > SF.ctime && SF.ctime == SF.mtime then, target file was modified, source file was not,
    `add` subcommand will copy new version of the target file to the source file (no conflict), `pull` subcommand
    will overwrite new changes in the target file (conflict).
    4. if TF.mtime > SF.ctime && SF.ctime < SF.mtime then, both files was modified independently, both `add` and
   `pull` subcommands will overwrite new modifications (conflict).

### Init
The supposed workflow is this:
Setting up an existing repo with dotfiles:
- user downloads the repository, with the source directory locating at the root of
the repository, or a one of its subdirectories.
- user executes `$ dfm init path/to/repo/` or with path directly to the source directory.
- user executes `$ dfm pull` to copy all the dotfiles to the home directory.

Creating a new repo for dotfiles:
- user crates a directory somewhere in filesystem to make it a source directory.
- user executes `$ dfm init path/to/that/new/dir`.
- user executes `$ dfm add` to add all dotfiles under the management.

The given path expected to be the source directory path.
- If the given path does not exist then exit with error.
- If this path contains a file `.dfm_root`, then the program reads the file content,
the content is a path to the source root.
- If the path from the `.dfm_root` does not exist then exit with error.
Recursively search for the source directory, by this way.
- Having source directory, search the config file of the program inside of it,
apply the `pull` subcommand to the found config file.
- If the config file does not exist in source directory then create the config
file in the `$XDG_CONFIG_PATH` (or `$HOME`?) directory and fill with default
config parameters from the call of `default_config` function.
- In the config file in the target directory, we must set the `source_dir` variable
to the path of the source directory.
- Create the empty `$XDG_STATE_PATH/dfm/state.toml` file if it does not exist or
clean the file if exists.
- Create a file `.dfm_root` with content "." if not exists in the source directory.

### Add
The subcommand take the paths of the target directory (does not operate on paths in the source directory, unlike
the other commands) and creates corresponding files in the source directory.
Subcommand traverses in depth all given path to locate the files, each file can be:
- a symlink, that points not into the source directory
    - if --force then create a symlink file in the source directory,
    - otherwise do nothing
- a symlink, that points into the source directory pointing at the corresponding file
    - do nothing
- a symlink, that points into the source directory pointing at the non-corresponding file
    - if --force then create a symlink file in source directory
- a symlink, that has an associated symlink file in the source directory
    - check if the symlink and the symlink file are pointing to the same file,
    if not, update the symlink file to point to the same file as tye symlink.
- a symlink, that has no associated symlink file in the source directory
    - if --force then create a symlink file.
- an existing file, that has no corresponding file in the source directory
    - create a corresponding file.
- an existing file, that has a corresponding file in the source directory
    - if changes in target and source files are not conflicting then
    copy target file to the source file
- a non-existing file, that has no corresponding file in the source directory
    - error "file not found"
- a non-existing file, that has a corresponding file in the source directory
    - error "file not found"

### Pull
The subcommand takes the names of a files from target directory.
If the specified filename does not exist in target directory, then `pull` will calculate the corresponding names
in the source directory. If there is no such a file in source directory - error.
For existing target files: replacement, for non-existing files: creation (does not require special conditions).
Replacement checks if there is no conflict.
Traverse all directories in given paths, get the list of files to work on
each file in the target directory could be:
- a symlink, that points not into the source directory
    - exit with error, or remove if --force?
- a symlink, that points into the source directory pointing at the corresponding file
    - do nothing
- a symlink, that points into the source directory pointing at the non-corresponding file
    - exit with error, if --force then remove the link and create one pointing to the right file
- a symlink, that has an associated symlink file in the source directory
    - if the link points to the file specified in the source symlink file then do nothing
    - otherwise error or if --force then recreate  the link.
- an existing file, that has no corresponding file in the source directory
    - error "target file is not managed"
- an existing file, that has a corresponding file in the source directory
    - if target file was not modified then overwrite it with the source file,
    - otherwise error or if --force then overwrite or if --merge call merge tool
- a non-existing file, that has no corresponding file in the source directory
    - error "file not found and not managed"
- a non-existing file, that has a corresponding file in the source directory
    - copy the source file to the path of a target file

The `pull` subcommand is able to take a path from the source directory,
to make is easier to copy just cloned files, that don't yet exist in the home directory.
Each file in the source directory could be:
- an existing file, that has no corresponding file in the target directory
    - copy file from source directory to the path of the target file
- an existing file, that has a corresponding file in the target directory
    - if target file is not modified then copy source file to the path of the target file
    - or error or if --force then copy, or is --merge then run merge
- an existing file, that has a corresponding symlink in the target directory
    - do nothing, but if the symlink pints to the wrong file recreate it if --force
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
    - otherwise ask for the --force flag
- a symlink, has no associated symlink file in the source directory
    - do nothing
- an existing file, that has no corresponding file in the source directory
    - do nothing
- an existing file, that has a corresponding file in the source directory
    - if the corresponding source file was not modified then remove it
    - otherwise ask for the --force flag
- a non-existing file, that has no corresponding file in the source directory
    - do nothing
- a non-existing file, that has a corresponding file in the source directory
    - if corresponding file was not modified then remove it
    - otherwise ask for the flag --force

The `forget` subcommand is able to take a path from the source directory,
to make is easier to remove files.
Each file in the source directory could be:
- an existing file, that has no corresponding file in the target directory
    - if source file ws not modified then remove it
    - otherwise ask for the flag --force
- an existing file, that has a corresponding file in the target directory
    - if the source file was not modified or both files was not modified then
    remove source file.
    - or ask for the flag --force to remove the source file.
- an existing file, that has a corresponding symlink in the target directory
    - check if symlink points to the same pointee as the source symlink file then
    remove the source symlink file
    - otherwise ask for the flag --force to remove it
- a non-existing file
    - error "file does not exist"

### Ignore
The program supports an ignore list for files in target and source directories.
Those ignore lists are files `~/.local/state/dfm/ignore_list` containing
ignored file paths and patterns for target directory and `**/dotfiles/.dfm_ignore_list`
containing file paths and patterns for the source directory. If a file contained in
the ignore list, it is not processed by other subcommands until the file was removed
from the ignore list.  
Ignore list consists of lines, each line is a comment (`#`) or a filepath or a regular
expression. Everything after `#` is ignored. `#` can be escaped `\#`, then it is a part
of the filepath or the regular expression.
- Blank lines ignored.
- A regular expression `abc#foo` will be read as `abc`, whereas `abc\#foo` will be read
as `abc#foo`.
- for each file in target directory the relative filepath is calculated. For file
`/home/user/.config/prg/config.yaml` the relative path will be `.config/prg/config.yaml`.
- the regular expression must match this relative path from the start to the end for
file to be ignored.

If regular expression matched to the path, the directory containing a file, then this
directory is not traversed - all files in it considered to be ignored.
