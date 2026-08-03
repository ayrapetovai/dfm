# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## A. Confirmed functional bugs (reproduced)

### A1. [SEVERE] `dfm pull` after `add -s` destroys the managed symlink
`src/commands/pull.rs:145-152` — `handle_source_path`

When the target is a symlink and the source is an ordinary **data** file (the case
produced by `add -s`, where the symlink *is* the managed file and the source holds the
real content), the branch treats the source file's *content* as a symlink pointee:

```rust
} else if target_file_abs_path.is_symlink() && source_file_abs_path.exists() {
    let target_symlink_pointee = fs::read_link(&target_file_abs_path)?;
    let source_file_content = read_symlink_pointer(source_file_abs_path)?; // reads file CONTENT
    if source_file_content != target_symlink_pointee.to_string_lossy().as_ref() {
        tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
```

Reproduction:
```bash
dfm init dotfiles
echo "content" > file.txt
dfm add -s file.txt      # file.txt -> dotfiles/file.txt (correct)
dfm pull                 # file.txt now -> "content" (dangling!)
readlink file.txt        # => "content"
```
After `pull`, `file.txt` points to a non-existent path named `content`. The data is still
in `dotfiles/file.txt`, but the user's file is broken. The branch should only fire when
the **source name ends with `symlink_postfix`** (a real pointer file), i.e. `read_symlink_pointer`
must not be called on plain data files. The README's pull table says "Symlink points to the
correct source file → Do nothing", which is the opposite of the observed behavior.

No test covers `add -s` followed by `pull` (only `test_purge_symlink_add_s.sh` uses `add -s`).

### A2. `dfm pull <target-path>` fails on an already-correct `add -s` symlink
`src/commands/pull.rs:226-230` — `handle_existing_target`

```rust
if target_symlink_followed_abs_path == source_file_abs_path {
    info!("target symlink ... points to the source file ..., skipping...");
    error_list.push(format!("target {:?} is a valid symlink", target_abs_path)); // ???
    return Ok(());
}
```
An *idempotent no-op* ("symlink is correct") is recorded as an **error**, so
`dfm pull file.txt` exits 1 with `improper operation` (and requires `--force`). Same repro
as A1 but with an explicit target path. The `error_list.push(...)` line is wrong — a valid
symlink should be silently skipped like every other up-to-date case.

### A3. `dfm forget --dry-run` mutates the state file
`src/commands/forget.rs:403-412` (Phase 2) + `src/main.rs:391-399` (`with_state_even_if_error`)

Phase 1 and 3 correctly skip deletions on dry-run, but Phase 2 removes state entries
**unconditionally**, and `with_state_even_if_error` always persists:

```rust
for task in &tasks {
    match task {
        ForgetTask::Delete(source_file) => { remove_sync_state(state, ...); },
        ForgetTask::RemoveState(key) => { state.syncs.remove(key); },
    }
}
```

Reproduction:
```bash
dfm init dotfiles; echo c > file.txt; dfm add file.txt
dfm forget --dry-run file.txt
grep file.txt .local/state/dfm/state.toml   # => entry GONE
```
A dry-run that deletes permanent state is a contract violation. The existing
`test_forget_dry_run.sh` only asserts the *files* still exist — it never checks the state
file, so it passes.

### A4. Path round-trip corrupts any component containing `dot_` as a substring
`src/commands/mod.rs:100` — `source_rel_to_target_rel`

```rust
let mut target_rel = source_rel.replace(dot_prefix, ".");   // replaces EVERYWHERE, not just leading dot
```
Encoding (`filepath_in_source_dir`, `lib.rs:650-686`) rewrites **only leading dots per
component**; decoding uses a blanket substring replace → the two are asymmetric.

Reproduction:
```bash
dfm init dotfiles
mkdir -p .config/dot_backup; echo x > .config/dot_backup/notes.txt
dfm add .config/dot_backup
dfm status --short          # => "!? .config/.backup/notes.txt"   (WRONG: .config/dot_backup/notes.txt)
```
The target `~/.config/dot_backup/notes.txt` decodes to `~/.config/.backup/notes.txt` — a
different location; a subsequent pull would place the file in the wrong directory. Fix: decode
component-wise (mirror of the encoder) instead of `str::replace`. Same `replace` misuse at
`src/commands/ignore.rs:92`.

### A5. One broken symlink aborts the entire `dfm add .`
`src/commands/add.rs:96-100` — `handle_target_symlink`

```rust
let target_symlink_pointee_abs_path = fs::canonicalize(&target_symlink_pointee_rel_path)
    .map_err(|e| DfmError::Other(format!("Symlink ... points to ... which does not exist: {}", e)))?;
```
For a broken symlink this returns a non-permission `Err`, and the add loop
(`add.rs:379`, `393`) does `Err(e) => return Err(e)` — aborting the whole traversal and
leaving *nothing* added. Contrast with `status` (`status.rs:450`), which handles broken
symlinks via `.ok()`. A dangling symlink anywhere in `$HOME` makes `dfm add .` useless.
Recommendation: skip broken symlinks with a `warn!` during traversal, like the
permission-denied path; error only when the user names the symlink explicitly.

### A6. Missing config file makes every state-dependent command fail
`src/lib.rs:593-623` — `merge_settings`

```rust
match custom_opt {
    Some(custom) => { ...source_dir/target_dir from state... }
    None => default.clone()          // source_dir = "", target_dir = "$HOME"
}
```
The source/target dirs live in the **state** file (by design), but they are only read when a
**config** file is also present. Delete just the config (e.g. `rm -rf ~/.config/dfm`) while
the state is intact and `add`/`pull`/`status` all fail:

```
dfm add .config/dot_backup; rm -rf .config   # also removes config.toml!
dfm pull
failed to read source path from the config file: empty string
```
The misleading message ("from the config file") compounds the confusion — the dirs never
came from the config. Fix: take dirs from state regardless of whether a config exists,
merging config only for the tunable fields.

### A7. `-c/--config` flag is parsed but never used
`src/main.rs:39-40`

```rust
/// Use other config.
#[arg(long, short = 'c', ...)]
config: Option<PathBuf>,
```
`args.config` is never referenced in `main_logic()` (lines 250-371). Verified:
`dfm -c /tmp/alt.toml paths` still prints the default config path. The flag is dead — either
wire it through (read state/config from the alternate path) or remove it from the CLI.

