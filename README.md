# Dotfile Manager (dfm)

A CLI tool to manage dotfiles: keep copies of configuration files from your home directory (**target**) inside a version-controlled **source** directory, and synchronize changes between them safely. `dfm` is **purely local** — it only ever moves files between the target and source directories on the same machine; no remote, host, or server mode, and it never talks to git servers, the network, cloud, or any external service (see [Never be in scope](#never-be-in-scope)).

- Copy files between target and source with conflict detection
- Three-way merge for conflicting files
- Symlink tracking (files and their pointees)
- Argon2id + XChaCha20-Poly1305 encrypted storage for sensitive files
- Ignore lists for target and source files

## Quick start

```bash
dfm init /path/to/dotfiles/repo && dfm pull     # existing dotfiles repo

git clone url-to-repo/dotfiles                  # or start fresh
dfm init dotfiles
dfm ignore .local .cache                        # ignore local dependent files
dfm add ~/.bashrc ~/.config/git/config
cd dotfiles && git add . && git commit -m "initial" && git push
```

## Dependencies

`dfm` is a self-contained Rust binary — **no mandatory runtime dependencies**; the tools below are optional:

| Tool | Required for |
|---|---|
| `less` (or `$PAGER`) | paged `status` output (default `less -FRSX`; falls back to plain stdout). |
| `git` | the git-info line in `status` (branch + dirty state; silently skipped if the source is not a git repo). |
| `sh` | `obtain_password_shell_command` (piped to `sh` stdin, not visible in `ps`); else interactive password prompt. |
| a merge tool (`vimdiff` default) | `merge`, via `merge_tool_command` (`{target}`, `{source}`, `{result}` placeholders). |
| a diff tool (`vimdiff` default) | `diff`, via `diff_tool_command` / `diff_all_tool_command_target` / `diff_all_tool_command_source` (`{target}`, `{source}` placeholders). |

## Table of Contents

1. [Concepts](#1-concepts)
2. [Commands](#2-commands)
3. [Configuration](#3-configuration)
4. [Encryption](#4-encryption)
5. [Conflict detection](#5-conflict-detection)
6. [File layout](#6-file-layout)

## 1 Concepts

| Term | Description |
|---|---|
| **Target directory** | root for managed files, usually `$HOME`. |
| **Source directory** | a directory (typically version-controlled) holding copies of managed files. |
| **Target file (TF) / Source file (SF)** | the managed file inside the target / its backing copy in the source. |
| **State file** | `$XDG_STATE_HOME/dfm/state.toml`, mapping each managed file to its sync time. |
| **Sync time** | the timestamp (`"<secs>;<nanos>"`) recorded when the last `add`/`pull` copy completed — the basis of conflict detection. |

### Path mapping

Target names starting with `.` get the `dot_prefix` (default `dot_`) in the source: `~/.bashrc` → `source_dir/dot_bashrc`, `~/.config/foo.conf` → `source_dir/dot_config/foo.conf`. Prefixes and postfixes are configurable ([Configuration](#3-configuration)).

## 2 Commands

Relative `PATH` arguments follow shell semantics: anchored at the **current working directory** and normalized lexically. A path resolving outside the managed tree (neither target nor source) is rejected; the read-only `diff` is the exception, reporting the path as *not managed*.

### 2.1 `init`

```bash
dfm init <PATH> [TARGET]
```

Creates the source directory (marker `.dfm_root`, searched upward in parents), the source ignore file, the state file at `$XDG_STATE_HOME/dfm/state.toml`, and the config at `$XDG_CONFIG_HOME/dfm/config.toml` with defaults. `TARGET` defaults to `$HOME`. `init` never looks for a config inside the source directory.

| Flag | Description |
|---|---|
| `-n`, `--dry-run` | Show what would be done without making changes. |

### 2.2 `add`

```bash
dfm add [PATH...] [--force] [--symlink] [--encrypt] [--dry-run]
```

Copies target files into the source. Without `PATH`, traverses the whole target (fully-ignored directories are pruned). Every file is checked against its source via [conflict detection](#5-conflict-detection); conflicts require `--force`.

| Flag | Description |
|---|---|
| `-f`, `--force` | Overwrite the source on conflict; also bypasses ignore patterns (the matching pattern is removed from the ignore file on success). |
| `-s`, `--symlink` | Move the file to the source and replace the target with a symlink. |
| `-e`, `--encrypt` | Encrypt the file in the source. |
| `-n`, `--dry-run` | Check without making changes. |

Symlinks: a traversed symlink becomes a `.symlink` pointer file recording its pointee; an existing pointer to a *different* pointee is updated; a pointee inside the source is handled as a regular file; `--force` always (re)creates the pointer.

### 2.3 `pull`

```bash
dfm pull [PATH...] [--force] [--symlink] [--dry-run]
```

Copies source files back into the target. Without `PATH`, pulls everything. A `PATH` may be a source path or a target path (mapped automatically).

| Flag | Description |
|---|---|
| `-f`, `--force` | Overwrite the target on conflict; also bypasses ignore patterns. |
| `-s`, `--symlink` | Create symlinks in the target pointing to source files. |
| `-n`, `--dry-run` | Check without making changes. |

Symlinks: with `--symlink`, a missing target gets a symlink (recreated when a pointee differs); a target symlink pointing to a *different* source file is an error unless `--force`; a symlink matching its source pointer file needs no action.

### 2.4 `merge`

```bash
dfm merge [PATH...]
```

Runs the merge tool on conflicting files. Without args: only `BothModified` files; with a `PATH` (target or source), merges even a single-side modification. Skips symlinks and ignored files.

`merge_tool_command` (default `vimdiff {target} {source} {result}`): `{target}` and `{source}` are the two sides (source decrypted if encrypted), `{result}` is the file the tool must write. On success the result is copied to both sides (re-encrypted if needed) and the sync state is updated.

### 2.5 `diff`

```bash
dfm diff [PATH...] [-a|--all] [-e|--editable]
```

Shows changes between a target file and its source; **read-only** except `--editable`. Without arguments (or with `-a`), batch mode diffs every *modified* managed file using the non-interactive `diff_all_tool_command_target`/`..._source` templates, concatenates the output, and pages it like `status`; up-to-date and never-synced files produce nothing. Explicit `PATH`s always use the per-path mode (`diff_tool_command`). Per path it reports:

| Situation | Output |
|---|---|
| synchronized | `{path} is synchronized` |
| target has no source | `{path} is not managed` |
| exists nowhere | `{path} does not exist` |
| source exists, target missing | `{corresponding_target_path} is not pulled` |
| matches an ignore pattern | `{path} is ignored by {regexp}` |
| target is a symlink | the diff of the pointees |
| target and source differ | the diff tool is run |

Modification is detected like `add` (mtime, then content hash). The tool is spawned directly (no shell); a missing tool fails with exit 1. `diff_tool_command` defaults to `vimdiff -M {target} {source}` (`-M` = read-only). For encrypted sources the *decrypted* plaintext is passed as a temporary file substituted for `{source}` — never on stdin, never the `.encrypted` bytes. Batch mode shows the modified side as the *new* side, decrypting encrypted sources to a transient scratch dir removed afterwards.

#### Editable diff

`diff --editable PATH...` (`-e`) edits both sides at once. It requires `PATH` and conflicts with `--all`; it works on already-diverged pairs (no equality check). It runs `diff_editable_tool_command` (default `vimdiff {target} {source}`, writable) on private copies in `.current_diff`. On exit **0** every changed side is written back independently — still-differing sides are written but not recorded as synchronized (the state updates only when both saved files are equal). On a **non-zero** exit (`:cq`) the edit is discarded and nothing is written. `--dry-run` only prepares the copies. A symlink, unmanaged, ignored, or un-pulled path is an error here, unlike the reporting modes.

### 2.6 `forget`

`dfm forget [PATH...] [--force] [--dry-run]` removes files from management — it **never deletes the target file**. Without a path it processes all managed files.

| Scenario | Behavior |
|---|---|
| symlink → correct source | remove the source file and the symlink |
| symlink → different source | remove the symlink only |
| source modified | require `--force` |
| source entry, no target file | remove the state entry (modified source → `--force`) |
| path exists nowhere | error `{path} does not exist`, exit 1, nothing forgotten |

### 2.7 `ignore`

`dfm ignore [PATH...] [-p PATTERN...] [-r RECORD...] [--dry-run]` adds paths or regex patterns to the ignore list (ignored files are skipped by `add`, `pull`, `merge`, `forget`). The three input groups are mutually exclusive and at least one is required. Adding a directory writes the directory itself, which is then pruned during traversal.

- **Target ignore** — `$XDG_STATE_HOME/dfm/ignore_file` (target-side patterns).
- **Source ignore** — `source_dir/.dfm_ignore_file` (source-side patterns).

Format: one entry per line; `#` starts a comment (`\#` escapes a literal `#`); blank lines are skipped; each entry is a regex matching the *full* relative path.

### 2.8 `paths`

`dfm paths` prints the resolved target, source, config, and state file paths.

### 2.9 `config`

```bash
dfm config --get <NAME> | --set <NAME> <VALUE> | --list | --default
```

`--get` prints a value, `--set` sets one, `--list` lists all, `--default` prints the default configuration as TOML (works before `init`, redirectable into the config file). Note: `dfm config list` (a positional) is invalid syntax.

The only array property is `force_encryption_for`; it uses the array syntax with `--set`:

| Value | Effect |
|---|---|
| `add:<element>` | append (a regex; must be non-empty) — also repairs a corrupted non-array value |
| `rm:<element>` | remove every equal element; error + exit 1 if none |
| `rmi:<index>` | remove the element at the 0-based index; error + exit 1 if invalid |

Unknown property names are rejected (before syntax checks) with exit 1; array syntax on a scalar or a plain value on the array property also errors with exit 1. An element literally starting with `add:`/`rm:`/`rmi:` needs a direct TOML edit. An emptied array means "use the default rule", not "no encryption".

### 2.10 `purge`

`dfm purge [--keep-source] [--keep-config-file] [--force] [--dry-run]` removes all program data: config file, source directory, and state directory. It aborts (unless `--force`) if there are un-pulled or un-pushed changes. The config's parent directory is removed only when the config is the default one (never `$HOME`); a custom `-c PATH` removes just the file. Managed symlinks are replaced by regular copies of their pointees before the source is removed; outside symlinks are left untouched.

### 2.11 `encrypt` / `decrypt`

`dfm encrypt [PATH] [-o OUTPUT]` / `dfm decrypt [PATH] [-o OUTPUT]` encrypt or decrypt a single file outside the target/source workflow (see [Encryption](#4-encryption)). `encrypt` defaults to `<input>.encrypted`; `decrypt` strips the `.encrypted` suffix (an explicit `-o` is required when the input has none).

### 2.12 `status`
`dfm status [OPTIONS] [PATH...]` shows the state of managed, unmanaged, ignored, and encrypted files. By default: a grouped, paged report of **modified + unmanaged** entries. `PATH` arguments restrict the report to those paths (ignored entries inside that scope are then shown even without a flag).

#### Status codes

| Code | Meaning |
|---|---|
| `--` | up to date |
| `MM` | both modified (conflict) |
| `M ` | target modified only |
| ` M` | source modified only |
| `NM` | never synchronized |
| `!?` | unpulled (target missing, source exists) |
| `??` / `?L` | unmanaged file / symlink |
| `LL` | managed symlink |
| `!!` / `!L` | ignored file / symlink (fully-ignored dir → one `!! dir/`) |
| `!P` | stale pattern (`--unused-patterns`) |

Two characters per code: **target** side first, **source** side second; ` ` = no change on that side.

#### Formats, filters

| Flag | Effect |
|---|---|
| *(default)* | grouped + paged report (`$PAGER`/`less`); modified + unmanaged only |
| `-s` / `--short` | one line `<code> <path>`; no pager |
| `--porcelain` | `<code>\t<path>`; stable, machine-readable, no pager |
| `-a` / `--all` | also up-to-date and ignored entries (hidden by default) |
| `-c` / `--conflicted` | only `MM` |
| `-m` / `--modified` | only target- or source-modified |
| `-U` / `--unmanaged` | only `??` / `?L` |
| `-M` / `--managed` | only tracked entries (implies `--all`) |
| `-p` / `--unpulled` | only `!?` |
| `-e` / `--encrypted` | only encrypted sources; overrides other filters, suppresses the stale block |
| `-i` / `--ignored` | only `!!` / `!L` |
| `-l` / `-u` | the active / the stale ignore patterns (no file entries) |

Filters combine **additively** (union — no priority, no contradictory-flag error); `-e` is the one override. The default report shows the git-info line (`git -C <source> status --porcelain -b`), folds directories as `dir/*`, right-aligns an `(encrypted)` marker on encrypted entries (`dir/* (encrypted)` for a fully-encrypted folded dir; never in `--short`/`--porcelain`), and includes the unused-patterns block (which any of the filter flags drops).

### 2.13 `sync`

`dfm sync [PATH...] [--force] [--dry-run]` synchronizes only **already-managed** files (target + source + a sync record): target-only changes are pushed, source-only changes pulled. Both-side changes are conflicts. Without `--force`: traverses all eligible files and exits non-zero when conflicts exist (nothing modified). With `--force`: copies all eligible (non-conflicting) files, never touches a conflict, exits 0. Unmanaged, never-synced, unpulled, and ignored files are never touched (ignored regardless of `--force`).

## 3 Configuration

The config file is `$XDG_CONFIG_HOME/dfm/config.toml` (fallback `~/.dfm.toml` when the XDG path is absent). `dfm config --default` prints the values below as TOML, redirectable into the config file.

```toml
dot_prefix = "dot_"
symlink_postfix = ".symlink"
encrypted_postfix = ".encrypted"
force_encryption_for = ["\\.ssh"]
obtain_password_shell_command = ""
merge_tool_command = "vimdiff {target} {source} {result}"
diff_tool_command = "vimdiff -M {target} {source}"
diff_all_tool_command_target = "diff -u --color=always {source} {target}"
diff_all_tool_command_source = "diff -u --color=always {target} {source}"
diff_editable_tool_command = "vimdiff {target} {source}"
```

| Property | Type | Purpose |
|---|---|---|
| `dot_prefix` | string | replaces a leading `.` in source filenames |
| `symlink_postfix` | string | suffix of symlink pointer files |
| `encrypted_postfix` | string | suffix of encrypted source files |
| `force_encryption_for` | array of regex | paths always encrypted on `add` |
| `obtain_password_shell_command` | shell command | command producing the encryption password |
| `merge_tool_command` | template | `{target}` / `{source}` / `{result}` |
| `diff_tool_command` / `diff_all_tool_command_*` | template | per-path diff / the two batch `--all` templates (`{target}` / `{source}`) |
| `diff_editable_tool_command` | template | `diff --editable`; must write both files |

The source and target directories are **not** config settings — they live in the state file. The config file is ordinary user data (manage it like any dotfile, e.g. `dfm add ~/.config/dfm`); the state and target-ignore files are internal.

## 4 Encryption

Sensitive files can be stored encrypted. Files matching `force_encryption_for` (default `\.ssh`) are auto-encrypted on `add`; `--encrypt` forces it per-run.

Each `.encrypted` file is a self-contained container:
- Argon2id password stretching (memory-hard KDF).
- XChaCha20-Poly1305 AEAD: tampering and wrong passwords are detected.
- 64 KiB chunks with per-chunk nonces and tags; reorder/dup/truncate/splice all fail authentication; peak RAM = one chunk.
- Filename, permissions, and directory structure are encrypted with the content.
- KDF cost parameters travel inside the archive, so defaults may change without breaking old files.

Format version 3; v1/v2 archives are rejected and must be re-created. Encryption/decryption is transparent during `add`/`pull` (and `merge`/`purge` on encrypted sources); streaming, no size cap.

### Obtaining a password

`obtain_password_shell_command` (default empty) is piped to `sh` stdin — **not** `-c` — so it never appears in `ps aux`. Example: `obtain_password_shell_command = "security find-generic-password -w -a dfm"`. When empty, dfm prompts interactively with masked input (`rpassword`). The password is cached for the process duration.

### Standalone `encrypt` / `decrypt`

`dfm encrypt path/to/file [-o output.encrypted]` writes `<input>.encrypted` next to the input by default. `dfm decrypt file.encrypted [-o output]` strips the `.encrypted` suffix (an explicit `-o` is required when the input has none) and restores the recorded permissions. Same password rules as above; no external tool required.

## 5 Conflict detection

Before any copy, dfm compares the **TF mtime**, **SF mtime**, and the stored **sync time** (from the last `add`/`pull`):

| Condition | Result | `add` | `pull` |
|---|---|---|---|
| TF == sync == SF | NonModified | skip (copy with `--force`) | skip (copy with `--force`) |
| TF == sync < SF | SourceModified | overwrite source (conflict) | copy source → target (safe) |
| TF > sync == SF | TargetModified | copy target → source (safe) | overwrite target (conflict) |
| TF > sync < SF | BothModified | conflict; use `merge` | conflict; use `merge` |
| no sync recorded | NeverSynchronized | record sync if equal; else `--force` | require `--force` |

Encrypted sources are compared by the encrypted file's mtime (re-encryption changes bytes); decryption is scheduled only when safe (or forced). `merge` handles encrypted sources by decrypting, merging, re-encrypting.

### All-or-nothing per-run semantics

`add`/`pull`/`merge`/`diff --editable` are atomic **per file**, not per run: copies already made during a failed run are not rolled back, but the sync state is committed only when the whole command succeeds (`with_state`), so a later run re-evaluates every file. `forget`/`purge` are best-effort and continue past per-file errors; `forget` persists state even on failure.

## 6 File layout

```
$XDG_CONFIG_HOME/dfm/config.toml       user config
~/.dfm.toml                            fallback config (if XDG path absent)
$XDG_STATE_HOME/dfm/state.toml         sync timestamps ("<secs>;<nanos>")
$XDG_STATE_HOME/dfm/ignore_file        target-side ignore patterns
source_dir/.dfm_root                   source dir marker
source_dir/.dfm_ignore_file            source-side ignore patterns
source_dir/dot_bashrc[.encrypted|.symlink]   managed / encrypted / pointer copy
source_dir/.current_merge|.current_diff       merge / diff scratch dirs (0700)
```
## Limitations
- **Root privileges**: refuses to run with root powers gained via sudo/setuid-style elevation (`DFM_ALLOW_ROOT=1` bypasses); a genuine root session still works.
- **Config `--set` arrays**: elements literally starting with `add:`/`rm:`/`rmi:` need a direct TOML edit.
- **UTF-8 only**: non-UTF-8 paths are unsupported.
- **Tools run without a shell**: `|`, `>`, `$VAR` in `merge_tool_command`/`diff_tool_command` are not processed.

## Never be in scope
- Windows support.
- Support of version management system other than git.
- Git commands embedding into CLI of dfm (dfm git status).
- Any remote/host/server, network, cloud, or other external service mode — synchronization is purely local.

## Repo management
```shell
git remote prune origin
git reflog expire --expire=now --all
git gc --prune=now --aggressive
```

