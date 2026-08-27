# Dotfile Manager (dfm)

A CLI tool to manage dotfiles: keep copies of configuration files from your home directory (**target**) inside a version-controlled **source** directory, and synchronize changes between them safely.

- Copy files between target and source with conflict detection
- Three-way merge for conflicting files
- Symlink tracking (files and their pointees)
- Argon2id + XChaCha20-Poly1305 encrypted storage for sensitive files
- Ignore lists for target and source files

## Quick start

```bash
# Initialize with an existing dotfiles repository
dfm init /path/to/dotfiles/repo
dfm pull

# Initialize with a new directory
git clone url-to-reop/dotfiles
dfm init dotfiles
dfm add ~/.bashrc ~/.config/git/config
cd dotfiles
git add .
git commit -m "initial"
git push
```

---

## Dependencies

`dfm` is a self-contained Rust binary — there are **no mandatory runtime dependencies**. The following tools are **optional**; each enhances a specific feature:

| Tool | Required for | Notes |
|---|---|---|
| — | Core functionality | Add, pull, forget, merge, diff, status, purge all work out of the box. |
| `less` (or `$PAGER`) | Paged `status` output | Default pager is `less -FRSX`. Falls back to plain stdout if unavailable. |
| `git` | Git-info line in `status` output | Shows branch and dirty state of the source directory. Silently skipped if the source directory is not a git repository. |
| `sh` (POSIX) | `obtain_password_shell_command` | The command is piped to `sh` stdin (not exposed in `ps`). Falls back to interactive password prompt when unset. |
| A merge tool (`vimdiff` by default) | `merge` subcommand | Configured via `merge_tool_command`. Any command that accepts `{target}`, `{source}`, `{result}` placeholders works (e.g., `vimdiff`, `nvim -d`, `meld`). |
| A diff tool (`vimdiff` by default) | `diff` subcommand | Configured via `diff_tool_command`. Any command that accepts `{target}`, `{source}` placeholders works (e.g., `diff -u`, `vimdiff`, `meld`). |

---

## Table of Contents

1. [Concepts](#1-concepts)
2. [Commands](#2-commands)
   - [init](#21-init)
   - [add](#22-add)
   - [pull](#23-pull)
   - [merge](#24-merge)
   - [diff](#25-diff)
   - [forget](#26-forget)
   - [ignore](#27-ignore)
   - [paths](#28-paths)
   - [config](#29-config)
   - [purge](#210-purge)
   - [status](#211-status)
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

Relative `PATH` arguments follow normal shell semantics: they are anchored at the **current working directory** and normalized lexically (`dir/../file` becomes `file`). A `PATH` may not resolve outside the managed tree: if it lands under neither the target nor the source directory (a `..` climbing past the managed root, or an absolute path elsewhere), the command rejects it with an error. The read-only `diff` is the exception — it accepts any existing path and reports it as *not managed*.

### 2.1 `init`

Set up the source directory, config file, and state file.

```bash
dfm init <PATH> [TARGET]
```

- `<PATH>` — the source directory (created if it does not exist). A marker file `.dfm_root` is written inside.
- `[TARGET]` — optional target directory. Default: `$HOME`.

`init` will:
1. Locate or create the source directory (recursively searches parent directories for `.dfm_root`).
2. Create the source ignore file if it does not exist (with `.dfm_root`, git-related entries, and the `.current_merge`/`.current_diff` temp dirs used by `merge`/`diff`).
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
| Symlink has an existing symlink file pointing to the correct pointee | Do nothing (already managed). |
| Symlink has an existing symlink file pointing to a *different* pointee | Update the symlink file to the current pointee. |
| Symlink has *no* symlink file and the pointee is outside the source directory | Create a symlink file pointing to the pointee. |
| Symlink has *no* symlink file and the pointee is inside the source directory | Do nothing (pointee is handled as a regular file). |
| `--force` | Always (re)create the symlink file using the current pointee, overriding the cases above. |

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

### 2.5 `diff`

Show the differences between a managed target file and its source using a diff tool.

```bash
dfm diff [PATH...]
```

- `PATH...` — files to diff. Pass a target path or a source path; the corresponding counter-part is resolved automatically (same as `pull`).
- Without arguments, `dfm diff` does nothing and exits successfully.
- `dfm diff` **never modifies any file** — it only reads.

For each path, `dfm diff` reports:

| Situation | Output |
|---|---|
| Target and source are synchronized | `{path} is synchronized` |
| Target file has no source file | `{path} is not managed` |
| Path exists neither in target nor in source | `{path} does not exist` |
| Source file whose target does not exist | `{corresponding_target_file_path} is not pulled` |
| Path matches an ignore pattern | `{path} is ignored by {regexp}` |
| Target is a symlink | The target's pointee and the source's pointee |
| Target and source differ | The diff tool is run |

Modification is detected the same way as in `add` (mtime, then content): only when the files actually differ in content is the diff tool invoked. A content-identical but differently-timestamped file is reported as synchronized.

The diff tool is configured by the `diff_tool_command` setting (default: `vimdiff -M {target} {source}` — `-M` makes both files unmodifiable, read-only). The placeholders are:

| Placeholder | Description |
|---|---|
| `{target}` | Target directory side (usually plain text). |
| `{source}` | Source directory side. When the source file is encrypted, the *decrypted* plaintext is passed to the tool as a temporary file (substituted for `{source}`); it is never piped to the tool's stdin (an interactive tool like `vimdiff` would read stdin into an extra buffer and refuse to quit), and the `.encrypted` bytes are never shown. |

Like the merge tool, the diff tool is launched directly (fork-exec, no shell). A missing diff tool makes `dfm diff` fail with exit code 1.

### 2.6 `forget`

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
| Path that exists nowhere (no target file, no source) | Error: `{path} does not exist` (exit code 1, nothing is forgotten). |

#### Source-path behavior

| Scenario | Behavior |
|---|---|
| Source file with no target file | Remove source (unless modified — require `--force`). |
| Source file with a target file | Remove source (unless modified — require `--force`). |
| Source symlink file with matching target symlink | Remove source symlink file. |
| Source symlink file with *mismatched* target symlink | Require `--force`. |
| Source path that does not exist and has no target either | Error: `{path} does not exist` |

### 2.7 `ignore`

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

### 2.8 `paths`

Print the resolved paths used by dfm.

```bash
dfm paths
```

Outputs the target directory, source directory, config file, and state file paths.

### 2.9 `config`

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

### 2.10 `purge`

Remove all program data: config file, source directory, and state directory.

```bash
dfm purge [--keep-source] [--keep-config-file] [--force] [--dry-run]
```

Before removing the source directory, `purge` checks for un-pulled changes (source files modified since their last sync) and un-pushed changes (target files modified since their last sync). If any exist, the command aborts unless `--force` is given.

The config file is removed (unless `--keep-config-file`). When the config file is the default one (at `$XDG_CONFIG_HOME/dfm/config.toml`), its parent directory is removed along with it; a config passed via `-c PATH` removes only the file, never the directory around it.

Managed symlinks (created by `add -s` / `pull -s`) are replaced with regular copies of the files they point to before the source directory is removed, so no dangling symlinks are left behind. Symlinks pointing outside the source directory are left untouched.

| Flag | Description |
|---|---|
| `-s`, `--keep-source` | Do not remove the source directory. |
| `-c`, `--keep-config-file` | Do not remove the config file. |
| `-f`, `--force` | Remove the source directory even if it has un-pulled or un-pushed changes. |
| `-n`, `--dry-run` | Check without making changes. |

### 2.11 `encrypt` / `decrypt`

Encrypt or decrypt a single file outside of the target/source workflow. See [Encryption](#4-encryption) for details on the format.

```bash
dfm encrypt [PATH] [-o OUTPUT]
dfm decrypt [PATH] [-o OUTPUT]
```

| Flag | Description |
|---|---|
| `-o`, `--output` | Output path. `encrypt` defaults to `<input>.encrypted`; `decrypt` strips the `.encrypted` suffix (an explicit `-o` is required when the input has no suffix). |

### 2.12 `status`

Show the current state of managed files, unmanaged files, and ignore patterns.

```bash
dfm status [--all] [--short] [--porcelain]
           [--conflicted] [--modified] [--unmanaged] [--managed] [--unpulled] [--ignored]
           [--ignored-patterns] [--unused-patterns]
           [PATH...]
```

By default, status prints a categorized report grouped by state. When one or more `PATH` arguments are given, the report is restricted to those paths only.

```
Up to date:
  --  .bashrc
  --  .config/git/config
  LL  .vimrc               (symlink, tracked)

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
| `!P` | Stale pattern — ignore pattern matches no files, shown by `--unused-patterns`. |

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

Without any filter flag, the default output shows: modified entries and unmanaged entries. Up-to-date (`--`, `LL`) and ignored (`!!`, `!L`) entries are **hidden** (use `--all` to see them). Exception: when the report is restricted to explicit `PATH` arguments, ignored entries inside that scope are shown even without a flag — naming a path asks "what is this file's state?", and *ignored* is the answer for those.

The "Unused ignore patterns" block is part of the **unfiltered** default report only (and of the dedicated `--unused-patterns` mode). Reports restricted by a filter flag show only their own lists and never include that block; `--all` keeps it.

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
diff_tool_command = "vimdiff -M {target} {source}"
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
| `diff_tool_command` | String (template) | Diff tool command with `{target}`, `{source}` placeholders. |

The source and target directories are **not** stored in the config file — they come from the state file (`state.toml`).

### Managing the config file itself

The dfm config file is ordinary user data: it can be managed like any other
dotfile with `add`, `pull`, `status`, `merge`, `diff`, and `forget` (e.g.
`dfm add ~/.config/dfm`). The state file (`state.toml`) and the target ignore
file are rewritten by dfm during sync runs, so they remain internal and cannot
be managed.

---

## 4 Encryption

Sensitive files can be stored encrypted. Files matching `force_encryption_for` regexes (default: `\.ssh`) are automatically encrypted on `add`. Encryption can also be requested per-run with `--encrypt`.

Each encrypted file (suffix `.encrypted`) is a self-contained, self-describing container:

- The password is stretched with **Argon2id** (memory-hard KDF), making brute-force attacks against weak passwords expensive.
- The payload is authenticated-encrypted with **XChaCha20-Poly1305** (AEAD): ciphertext tampering and wrong passwords are detected.
- The plaintext is sealed in **64 KiB chunks** (stream construction): each chunk carries its own Poly1305 tag, its nonce binds it to its position in the stream, and the declared total length is authenticated — so reordered, duplicated, truncated or spliced chunks fail authentication. Encryption and decryption hold at most one chunk in RAM regardless of file size.
- The **filename, file permissions, and directory structure are encrypted together with the content** — nothing about the payload is visible without the password.
- The KDF cost parameters travel inside the archive, so files decrypt correctly even if the default costs change in a future version.

Encrypted files use format version 3; older (v1/v2) archives are rejected with an "unsupported encrypted format version" error and must be re-created.

`dfm` performs encryption/decryption transparently during `add` and `pull` (and `merge` / `purge` for encrypted sources).

### Obtaining a password

When `obtain_password_shell_command` is set (non-empty), dfm pipes the command to `sh` stdin and reads the password from stdout. The command is **not** passed as a `-c` argument, so it does not appear in the process listing (`ps aux`). Example:

```bash
# Config
obtain_password_shell_command = "security find-generic-password -w -a dfm"
```

When the setting is empty (the default), dfm prompts interactively using `rpassword` (masked input with `*`).

The password is cached in memory for the duration of the process, so you are prompted only once per `dfm` invocation.

### Standalone `encrypt` / `decrypt`

Encrypted files can also be produced and inspected outside of the target/source workflow:

```bash
dfm encrypt path/to/file [-o output.encrypted]
dfm decrypt file.encrypted [-o output]
```

- `encrypt` writes `<input>.encrypted` next to the input by default (overridable with `-o`).
- `decrypt` strips the `.encrypted` suffix for the default output path; if the input has no suffix, an explicit `-o` is required.
- The same `obtain_password_shell_command` / interactive prompt rules apply. Decrypting also restores the file permissions recorded at encrypt time.

There is no external tool requirement — `dfm decrypt` (or a future re-encrypting `dfm add`) is the supported way to read `.encrypted` files.

### Memory use

Encryption and decryption are streaming: at most one 64 KiB plaintext chunk, its ciphertext, and the fixed-size header are held in RAM regardless of file size. There is no hard size cap.

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

### All-or-nothing per-run semantics

Each `add`, `pull`, or `merge` run is atomic **per file**, not per run:

- Files already processed remain applied if a later file fails mid-run.
- The sync state is written **only when the whole command succeeds** (state is committed at the end via `with_state`). A failed run persists no state, so a later run re-evaluates every file from scratch.
- `forget`/`purge` intentionally continue past per-file errors (best-effort), and `forget` persists state even on failure.

This means a multi-file sync is never left half-committed in the state file, even though individual copies made during a failed run are not rolled back.

---

## 6 File layout

```
$XDG_CONFIG_HOME/dfm/config.toml           -- user config
~/.dfm.toml                                -- fallback config (if XDG path absent)
$XDG_STATE_HOME/dfm/state.toml             -- sync timestamps (`"<secs>;<nanos>"` per file)
$XDG_STATE_HOME/dfm/ignore_file            -- target-side ignore patterns
source_dir/.dfm_root                       -- source directory marker
source_dir/.dfm_ignore_file                -- source-side ignore patterns
source_dir/dot_bashrc                      -- managed copy of ~/.bashrc
source_dir/dot_bashrc.encrypted            -- encrypted managed copy
source_dir/dot_bashrc.symlink              -- symlink pointer file
source_dir/.current_merge/                 -- transient merge-tool scratch dir (0700, removed on exit)
source_dir/.current_diff/                  -- transient diff-tool scratch dir (0700, encrypted diffs only)
```

---

## Limitations

- **Root privileges**: `dfm` refuses to run with root privileges it did not get as the root user itself (e.g. `sudo dfm` or a setuid-style elevation of a non-root user). A genuine root session (uid 0 launched by the root user itself) still works. Set `DFM_ALLOW_ROOT=1` to bypass the check.
- **Config `--set` and arrays**: Array-typed properties (`force_encryption_for`) cannot be set via `--set`; edit the TOML file directly.
- **Dotfiles outside UTF-8 paths**: Only valid UTF-8 paths are supported.
- **Merge tool**: The merge command is run directly (no shell), so shell features (`|`, `>`, `$VAR`) in `merge_tool_command` are not processed.
- **Diff tool**: Same as the merge tool — `diff_tool_command` is run directly (no shell), so shell features are not processed.

## Never be in scope
- Windows support.
- Support of version management system other than git.
- Git commands embedding into CLI of dfm (dfm git status).

## Repo management

To reclaim space (including deleted files) in a local git repo:

```shell
git remote prune origin
git reflog expire --expire=now --all
git gc --prune=now --aggressive
```

