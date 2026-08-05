# Dotfile Manager (dfm)

A CLI tool to manage dotfiles: keep copies of configuration files from your home directory (**target**) inside a version-controlled **source** directory, and synchronize changes between them safely.

- Copy files between target and source with conflict detection
- Three-way merge for conflicting files
- Symlink tracking (files and their pointees)
- AES-encrypted storage for sensitive files
- Ignore lists for target and source files

## Quick start

```bash
# Initialize with an existing dotfiles repository
dfm init /path/to/dotfiles/repo
dfm pull

# Initialize with a new directory
git clone link/to/new/dotfiles/repo
dfm init /path/to/new/dotfiles/repo
dfm add ~/.bashrc ~/.config/git/config
git add .
git commit -m "initial"
git push
```

---

## Dependencies

`dfm` is a self-contained Rust binary — there are **no mandatory runtime dependencies**. The following tools are **optional**; each enhances a specific feature:

| Tool | Required for | Notes |
|---|---|---|
| — | Core functionality | Add, pull, forget, merge, status, purge all work out of the box. |
| `less` (or `$PAGER`) | Paged `status` output | Default pager is `less -FRSX`. Falls back to plain stdout if unavailable. |
| `git` | Git-info line in `status` output | Shows branch and dirty state of the source directory. Silently skipped if the source directory is not a git repository. |
| `sh` (POSIX) | `obtain_password_shell_command` | The command is piped to `sh` stdin (not exposed in `ps`). Falls back to interactive password prompt when unset. |
| A merge tool (`vimdiff` by default) | `merge` subcommand | Configured via `merge_tool_command`. Any command that accepts `{target}`, `{source}`, `{result}` placeholders works (e.g., `vimdiff`, `nvim -d`, `meld`). |
| `7z` (p7zip) | Manual decryption of `.encrypted` files | Encrypted files are standard AES-256 ZIP archives and can be decrypted with any compatible tool. `7z` is recommended in the documentation. |

---

## Table of Contents

1. [Concepts](#1-concepts)
2. [Commands](#2-commands)
   - [init](#21-init)
   - [add](#22-add)
   - [pull](#23-pull)
   - [merge](#24-merge)
   - [forget](#25-forget)
   - [ignore](#26-ignore)
   - [paths](#27-paths)
   - [config](#28-config)
   - [purge](#29-purge)
   - [status](#210-status)
3. [Configuration](#3-configuration)
4. [Encryption](#4-encryption)
5. [Conflict detection](#5-conflict-detection)
6. [File layout](#6-file-layout)

---

## 1 Concepts

| Term | Description |
|---|---|
| **Target directory** | The root for all managed files, usually `$HOME`. |
| **Source directory** | A directory (typically under version control) that stores copies of managed files. |
| **Target file (TF)** | A managed file inside the target directory. |
| **Source file (SF)** | The backing copy inside the source directory. |
| **State file** | A TOML file (`state.toml`) that maps each managed file to a `"<seconds>;<nanos>"` sync timestamp. |
| **Sync time** | The timestamp recorded when a target→source (add) or source→target (pull) copy completed. Used for conflict detection. |

### Path mapping

File names starting with `.` in the target directory are stored with a **dot prefix** in the source directory (default: `dot_`). This keeps hidden files visible in the source tree.

- Target `~/.bashrc` → Source `source_dir/dot_bashrc`
- Target `~/.config/foo.conf` → Source `source_dir/dot_config/foo.conf`

The dot prefix and other postfixes are configurable (see [Configuration](#3-configuration)).

---

## 2 Commands

### 2.1 `init`

Set up the source directory, config file, and state file.

```bash
dfm init <PATH> [TARGET]
```

- `<PATH>` — the source directory (created if it does not exist). A marker file `.dfm_root` is written inside.
- `[TARGET]` — optional target directory. Default: `$HOME`.

`init` will:
1. Locate or create the source directory (recursively searches parent directories for `.dfm_root`).
2. Create the source ignore file if it does not exist (with `.dfm_root` and git-related entries).
3. Create or clear the state file at `$XDG_STATE_HOME/dfm/state.toml`, writing the target and source directory paths into it.
4. Create the config file at `$XDG_CONFIG_HOME/dfm/config.toml` with defaults if it does not exist.

`init` does not look for a config file inside the source directory; the config file always lives outside the source directory.

| Flag | Description |
|---|---|
| `-n`, `--dry-run` | Show what would be done without making changes. |

### 2.2 `add`

Copy files from the target directory to the source directory.

```bash
dfm add [PATH...] [--force] [--symlink] [--encrypt] [--dry-run]
```

- `PATH...` — files or directories to add. Omitting traverses the entire target directory (respecting ignore rules).
- Fully-ignored directories are pruned during the traversal: they are never descended into (so their files are not visited, counted in progress, or reported), and the matching pattern is kept. An explicitly named ignored path is still entered — combine with `--force` to add an ignored directory anyway.
- Each file is compared against its source counterpart using [conflict detection](#5-conflict-detection). Only safe copies proceed automatically; conflicts require `--force`.

| Flag | Description |
|---|---|
| `-f`, `--force` | Overwrite source files on conflict. Also bypasses ignore patterns (the matching pattern is removed from the ignore file on success). |
| `-s`, `--symlink` | Move the file to the source directory and replace the target with a symlink. |
| `-e`, `--encrypt` | Encrypt the file before storing in the source directory. |
| `-n`, `--dry-run` | Check without making changes. |

#### Symlink handling

When traversed paths include symlinks, `add` resolves each symlink using these rules:

| Scenario | Behavior |
|---|---|
| Symlink points *outside* the source directory | Create a symlink file in the source directory (with `--force` only). |
| Symlink points to the corresponding source file | Do nothing. |
| Symlink points to a *different* source file | Update the symlink file (with `--force` only). |
| Symlink has an existing symlink file in source | Update the symlink file if the pointee differs. |
| Symlink has *no* symlink file in source | Create a symlink file (with `--force` only). |

### 2.3 `pull`

Copy files from the source directory to the target directory.

```bash
dfm pull [PATH...] [--force] [--symlink] [--dry-run]
```

- `PATH...` — files or directories in the *source* directory. Omitting pulls all files from the source directory.
- You may also pass a target-directory path; the corresponding source path is computed automatically.

| Flag | Description |
|---|---|
| `-f`, `--force` | Overwrite target files on conflict. Also bypasses ignore patterns (the matching pattern is removed from the ignore file on success). |
| `-s`, `--symlink` | Create symlinks in the target directory pointing to source files. |
| `-n`, `--dry-run` | Check without making changes. |

#### Symlink handling (non-source path)

| Scenario | Behavior |
|---|---|
| Symlink points outside source dir | Error (or overwrite with `--force`). |
| Symlink points to the correct source file | Do nothing. |
| Symlink points to a *different* source file | Error (or fix with `--force`). |
| Symlink matches its source symlink file | Do nothing. |
| Symlink *differs* from its source symlink file | Recreate the symlink (with `--force` only). |

#### Symlink handling (source path)

| Scenario | Behavior |
|---|---|
| Source symlink file + target path does not exist | Create a symlink in the target. |
| Source symlink file + existing target symlink | Recreate if the pointee does not match. |

### 2.4 `merge`

Run the three-way merge tool on conflicting files.

```bash
dfm merge [PATH...]
```

- `PATH...` — optional paths to force-merge regardless of conflict state. Pass a target path or a source path; the corresponding counter-part is resolved automatically.
- Without arguments, scans all entries in the state file for `BothModified` files only.
- With a path given, merges the file even if only one side was modified — useful for resolving a `M ` or ` M` state proactively.
- Skips symlinks and files matching the target ignore pattern.

The merge tool is configured by the `merge_tool_command` setting (default: `vimdiff {target} {source} {result}`). The placeholders are:

| Placeholder | Description |
|---|---|
| `{target}` | Working-directory side (plain text copy). |
| `{source}` | Cellar side (decrypted if the source is encrypted). |
| `{result}` | Output file — the merge tool writes the result here. |

After the merge tool exits successfully, `result.<file>` is copied back to both the target and the source (and re-encrypted if needed). The sync state is updated to the merge time.

### 2.5 `forget`

Remove a file from management (does **not** delete the target file).

```bash
dfm forget [PATH...] [--force] [--dry-run]
```

- `PATH...` — paths in either the target or source directory.
- Without a path, `forget` processes all managed files.

#### Target-path behavior

| Scenario | Behavior |
|---|---|
| Symlink pointing outside source dir | Do nothing. |
| Symlink pointing to the correct source file | Remove both source file and symlink. |
| Symlink pointing to a *different* source file | Remove the symlink only. |
| Symlink matching its source symlink file | Remove the source symlink file. |
| Symlink *differing* from its source symlink file | Require `--force`. |
| File with a corresponding source file | Remove the source file (unless source was modified — then require `--force`). |
| File with no corresponding source file | Do nothing. |
| Non-existing file with a source entry | Remove the state entry (unless source was modified — then require `--force`). |

#### Source-path behavior

| Scenario | Behavior |
|---|---|
| Source file with no target file | Remove source (unless modified — require `--force`). |
| Source file with a target file | Remove source (unless modified — require `--force`). |
| Source symlink file with matching target symlink | Remove source symlink file. |
| Source symlink file with *mismatched* target symlink | Require `--force`. |

### 2.6 `ignore`

Add paths or regex patterns to the ignore list. Ignored files are skipped by `add`, `pull`, `merge`, and `forget`.

```bash
dfm ignore [PATH...] [-p PATTERN...] [-r RECORD...] [--dry-run]
```

- `PATH...` — file paths to ignore (relative to target or source directory).
- `-p`, `--patterns` — regex patterns to ignore.
- `-r`, `--remove` — records to remove from the ignore list.
- At least one of `PATH...`, `--patterns`, or `--remove` is required; running `dfm ignore` with none of them exits with an error.
- `PATH...`/`--patterns` add records to the ignore list, while `--remove` deletes them — the three are **mutually exclusive** and combining any of them is rejected by the CLI.
- When adding a directory path, dfm writes the directory itself to the ignore file; the directory (and everything under it) is then skipped by `add`, `merge`, `forget`, and `status` — ignored directories are pruned during traversal rather than walked and filtered file-by-file.

The program maintains two ignore files:
- **Target ignore file** at `$XDG_STATE_HOME/dfm/ignore_file` — patterns for target-side files.
- **Source ignore file** at `source_dir/.dfm_ignore_file` — patterns for source-side files.

Ignore file format:
- One entry per line.
- Lines starting with `#` are comments. `\#` escapes a literal `#`.
- Blank lines are ignored.
- Each line is a regex that must match the *full* relative path (from the root of the target or source directory).

### 2.7 `paths`

Print the resolved paths used by dfm.

```bash
dfm paths
```

Outputs the target directory, source directory, config file, and state file paths.

### 2.8 `config`

Read or write config file properties.

```bash
dfm config --get <NAME>
dfm config --set <NAME> <VALUE>
dfm config --list
```

| Flag | Description |
|---|---|
| `-g`, `--get <NAME>` | Print the value of a config property. |
| `-s`, `--set <NAME> <VALUE>` | Set a config property. |
| `-l`, `--list` | List all config properties. |

Note: Array-typed properties (`force_encryption_for`) cannot be set via `--set`; edit the config file directly.

### 2.9 `purge`

Remove all program data: config file, source directory, and state directory.

```bash
dfm purge [--keep-source] [--keep-config-file] [--force] [--dry-run]
```

Before removing the source directory, `purge` checks for un-pulled changes (source files modified since their last sync) and un-pushed changes (target files modified since their last sync). If any exist, the command aborts unless `--force` is given.

Managed symlinks (created by `add -s` / `pull -s`) are replaced with regular copies of the files they point to before the source directory is removed, so no dangling symlinks are left behind. Symlinks pointing outside the source directory are left untouched.

| Flag | Description |
|---|---|
| `-s`, `--keep-source` | Do not remove the source directory. |
| `-c`, `--keep-config-file` | Do not remove the config file. |
| `-f`, `--force` | Remove the source directory even if it has un-pulled or un-pushed changes. |
| `-n`, `--dry-run` | Check without making changes. |

### 2.10 `status`

Show the current state of managed files, unmanaged files, and ignore patterns.

```bash
dfm status [--all] [--short] [--porcelain]
           [--conflicted] [--modified] [--unmanaged] [--managed] [--unpulled] [--ignored]
           [--ignored-patterns] [--unused-patterns]
```

By default, status prints a categorized report grouped by state:

```
Up to date:
  --  .bashrc
  --  .config/git/config

Modified:
  MM  .ssh/config          (both target and source modified)

Unmanaged:
  ?L  .some_symlink        (symlink, not tracked)
  ??  temp.txt             (regular file, not tracked)

Ignore patterns:
  /\.swp$/
  *.log
```

#### Output formats

| Flag | Description |
|---|---|
| *(default)* | Grouped human-readable report with sections, paged through `$PAGER` (default `less`). |
| `-s`, `--short` | One line per entry: `<code> <path>` (no headers, no pager). |
| `--porcelain` | Tab-separated: `<code>\t<path>` (stable, machine-readable, no pager). |

#### Status codes

| Code | Meaning |
|---|---|
| `--` | Up to date — target and source are synchronized. |
| `MM` | **BothModified** — both target and source were modified since last sync (conflict). |
| `M ` | Target modified — only the target was changed since last sync. |
| ` M` | Source modified — only the source was changed since last sync. |
| `NM` | **NeverSynchronized** — both target and source exist but have never been synchronized. |
| `!?` | Missing target — the managed file's target path does not exist but the source does (unpulled). |
| `??` | Unmanaged — regular file exists in the target directory but is not tracked. |
| `?L` | Unmanaged symlink — symlink exists in the target directory but is not tracked. |
| `LL` | Managed symlink — symlink tracked via a `.symlink` pointer file. |
| `!!` | Ignored — file matches an ignore pattern. A fully-ignored directory is shown as a single `!! dir/` entry (with trailing slash) instead of its contents. |
| `!L` | Ignored symlink — symlink matches an ignore pattern. |

Codes are two characters: the first represents the **target** side, the second represents the **source** side. A space (` `) means "no change" on that side.

#### Filter flags

| Flag | Shows only |
|---|---|
| `-a`, `--all` | Show all entries, including up-to-date (`--`), managed-symlink (`LL`), and ignored (`!!`, `!L`) — all of which are hidden by default. |
| `-c`, `--conflicted` | Entries with `MM` (BothModified). |
| `-m`, `--modified` | Entries where target or source was modified. |
| `-U`, `--unmanaged` | Untracked files (`??`, `?L`). |
| `-M`, `--managed` | Tracked entries only (inverse of `--unmanaged`). Implies `--all` for managed files. |
| `-p`, `--unpulled` | Source-only entries (source modified, target missing). |
| `-i`, `--ignored` | Ignored files (`!!`, `!L`). |
| `-l`, `--ignored-patterns` | List active ignore patterns (no file entries). |
| `-u`, `--unused-patterns` | List ignore patterns that match no files. |

Without any filter flag, the default output shows: modified entries and unmanaged entries. Up-to-date (`--`, `LL`) and ignored (`!!`, `!L`) entries are **hidden** (use `--all` to see them).

---

## 3 Configuration

The config file is read from `$XDG_CONFIG_HOME/dfm/config.toml` (or `~/.dfm.toml` if the XDG path does not exist).

### Default settings

```toml
dot_prefix = "dot_"
symlink_postfix = ".symlink"
encrypted_postfix = ".encrypted"
force_encryption_for = ["\\.ssh"]
obtain_password_shell_command = ""
merge_tool_command = "vimdiff {target} {source} {result}"
```

### Properties

| Property | Type | Description |
|---|---|---|
| `dot_prefix` | String | Prefix to replace leading `.` in filenames inside the source directory. |
| `symlink_postfix` | String | Suffix appended to symlink pointer files in the source directory. |
| `encrypted_postfix` | String | Suffix appended to encrypted source files. |
| `force_encryption_for` | Array of regex | File paths matching these regexes are always encrypted on `add`. |
| `obtain_password_shell_command` | String (shell command) | Command to obtain the encryption password. See [Encryption](#4-encryption). |
| `merge_tool_command` | String (template) | Merge tool command with `{target}`, `{source}`, `{result}` placeholders. |

The source and target directories are **not** stored in the config file — they come from the state file (`state.toml`).

---

## 4 Encryption

Sensitive files can be stored in AES-encrypted ZIP archives. Files matching `force_encryption_for` regexes (default: `\.ssh`) are automatically encrypted on `add`. Encryption can also be requested per-run with `--encrypt`.

### Obtaining a password

When `obtain_password_shell_command` is set (non-empty), dfm pipes the command to `sh` stdin and reads the password from stdout. The command is **not** passed as a `-c` argument, so it does not appear in the process listing (`ps aux`). Example:

```bash
# Config
obtain_password_shell_command = "security find-generic-password -w -a dfm"
```

When the setting is empty (the default), dfm prompts interactively using `rpassword` (masked input with `*`).

The password is cached in memory for the duration of the process, so you are prompted only once per `dfm` invocation.

### Decryption manually

Encrypted files (suffix `.encrypted`) are standard ZIP archives with AES-256 encryption. They can be decrypted with any tool that supports AES-encrypted ZIP, such as `7z`:

```bash
# will ask for the password
7z x filename.encrypted
```

---

## 5 Conflict detection

Before any copy, dfm compares timestamps to detect concurrent modifications. The comparison uses three values:

- **TF mtime** — last modification time of the target file.
- **SF mtime** — last modification time of the source file.
- **Sync time** — the stored timestamp of the last successful sync (set by both `add` and `pull`).

The algorithm:

| Condition | Result | `add` behavior | `pull` behavior |
|---|---|---|---|
| TF mtime == sync == SF mtime | **NonModified** | Skip (or copy with `--force`) | Skip (or copy with `--force`) |
| TF mtime == sync < SF mtime | **SourceModified** | Overwrite source (conflict) | Copy source → target (safe) |
| TF mtime > sync == SF mtime | **TargetModified** | Copy target → source (safe) | Overwrite target (conflict) |
| TF mtime > sync < SF mtime | **BothModified** | Conflict; `dfm merge` to resolve | Conflict; `dfm merge` to resolve |
| No sync time recorded | **NeverSynchronized** | Record sync if content equal; require `--force` otherwise | Require `--force` |

For encrypted source files, the conflict check is performed against the encrypted file's mtime; decryption is only scheduled when safe (or forced). The `dfm merge` subcommand also handles encrypted sources by decrypting, merging, and re-encrypting the result.

---

## 6 File layout

```
$XDG_CONFIG_HOME/dfm/config.toml         -- user config
~/.dfm.toml                               -- fallback config (if XDG path absent)
$XDG_STATE_HOME/dfm/state.toml            -- sync timestamps (`"<secs>;<nanos>"` per file)
$XDG_STATE_HOME/dfm/ignore_file           -- target-side ignore patterns
source_dir/.dfm_root                       -- source directory marker
source_dir/.dfm_ignore_file                -- source-side ignore patterns
source_dir/dot_bashrc                      -- managed copy of ~/.bashrc
source_dir/dot_bashrc.encrypted            -- encrypted managed copy
source_dir/dot_bashrc.symlink              -- symlink pointer file
```

---

## Limitations

- **Config `--set` and arrays**: Array-typed properties (`force_encryption_for`) cannot be set via `--set`; edit the TOML file directly.
- **Dotfiles outside UTF-8 paths**: Only valid UTF-8 paths are supported.
- **Merge tool**: The merge command is run directly (no shell), so shell features (`|`, `>`, `$VAR`) in `merge_tool_command` are not processed.

## Building

### Install tools

```shell
# install https://rust-lang.org/tools/install/
cargo install cargo-aur
```

### Create a package from sources

```shell
cargo build --release
cargo aur
cd target/cargo-aur
makepkg
```

The package will appear in ./target/cargo-aur

### Install and remove

```shell
sudo pacman -U dfm-version-arch.zst
sudo pacman -R dfm-bin
```

