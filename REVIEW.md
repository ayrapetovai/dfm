# Code Review — dfm

Review date: 2026-08-02. Scope: all of `src/` (3532 lines), `tests/launcher.sh` (213 lines), `Cargo.toml`.
Builds clean, all 12 Rust unit tests pass. The shell suite (161 tests) was not re-run.

## Overview

| File | LOC | Verdict |
|---|---|---|
| `src/lib.rs` | 1207 | Solid core; 2 hand-rolled path helpers are overcomplicated, 1 dead field |
| `src/main.rs` | 390 | Repetitive dispatch boilerplate, `unwrap()` noise |
| `src/commands/status.rs` | 800 | Presentation mixed into data (causes 2 real bugs); one dense 180-line formatter |
| `src/commands/forget.rs` | 382 | Analysis loop is a 180-line nested if/else maze |
| `src/commands/pull.rs` | 432 | Source-path branch has hard-to-follow fall-through control flow |
| `src/commands/add.rs` | 433 | Dead/confusing code in symlink branch; per-file RegexSet rebuild |
| `src/crypt.rs` | 170 | **1 security issue (permissions), 1 footgun (password newline)** |
| `src/commands/{merge,init,ignore,purge,config,paths}.rs` | ~950 | Generally fine; merge leaks temp dir on error |

Priorities: **P0** = security/correctness, **P1** = overcomplicated/shrink, **P2** = hygiene.

---

## P0 — Security

### 1. Decrypted files lose their permissions (`crypt.rs`)
`write_zip_file` stores the source permissions in the ZIP entry (`unix_permissions`, crypt.rs:110), but `read_zip_file` (crypt.rs:146-150) creates the output with `File::create` → default `0666 & ~umask` (usually `644`) and never restores the entry's mode. `sync_file_copy` preserves permissions for plain copies, so only the encrypt/decrypt path loses them.

Consequence: `dfm pull` of a `.ssh` file (force-encrypted by default config) that was `600` becomes `644` — **private keys world-readable**. `purge`'s `replace_managed_symlinks` already restores perms (purge.rs:252-254), which shows the right pattern.

Fix: read `zip_file.unix_mode()` from the entry and `fs::set_permissions` the output.

### 2. Weak AES key derivation — PBKDF2 with 1000 iterations
The `zip` crate derives the AES-256 key with `pbkdf2::pbkdf2::<Sha1>(password, salt, ITERATION_COUNT)` where `ITERATION_COUNT = 1000` (verified in `zip-8.6.0/src/aes.rs`). 1000 iterations is far below the ~600k-1M recommended for a KDF; an offline brute-force of a weak password is feasible. This is the crate's default, not dfm's code, but since dfm's whole value proposition is "encrypted dotfiles", it should be surfaced in the README (or mitigated by wrapping the archive in an `age`-style envelope / using a stronger-derivation container).

### 3. Path traversal via tampered state keys
State keys are joined onto `source_dir` and passed through `remove_dots_from_path`, which resolves `..` lexically. A key like `../../.bashrc` in a tampered `state.toml` would, e.g., make `forget` orphan-processing (forget.rs:272-294) delete a file outside the source directory. The state file is user-owned so this is hardening rather than an active exploit, but a `syncs` key containing a `..` component should be rejected (or escaped) at `read_state` time.

### 4. Password from `obtain_password_shell_command` keeps trailing newline
`crypt.rs:73-74` takes the shell command's stdout verbatim. A command like `security find-generic-password …` outputs `pass\n`, so the stored password is `pass\n`. Encryption/decryption stay consistent with each other, but **manual `7z x` decryption with the intended password silently fails** (README documents manual decrypt as a supported workflow). Trim trailing `\r?\n` (and decide explicitly whether leading/trailing whitespace is part of the password).

---

## P0 — Correctness

### 5. Status stores ANSI color codes inside `StatusEntry.path` (`status.rs`)
Phase 1 colorizes paths at status.rs:210-214 and stores them in `entry.path`; the plain path is discarded. Two bugs follow:

- **`--porcelain` emits ANSI escapes** in a real terminal (`colored` colorizes whenever stdout is a TTY), violating the documented "stable, machine-readable" contract. `status.rs:460` has the same problem in the `unused_patterns` branch.
- **Phase 3 stale-pattern detection pattern-matches the colored string** (status.rs:423-429 feeds `entry.path` into `pattern_matches_path_components`). With color active, `^\.bashrc$` is tested against `\x1b[31m.bashrc\x1b[0m` and never matches → ignore patterns for modified/up-to-date managed files are **wrongly reported as unused**.

Fix: keep `path` raw in the entry; apply color only at render time (the `write_group`/short/porcelain output paths). This is a pure design-smell fix and makes the porcelain output deterministic.

### 6. `add`: force-encryption RegexSet rebuilt for every file
`add.rs:192` builds a `RegexSet` from `settings.force_encryption_for` inside the traversal loop → O(files × patterns) regex compilation. Hoist it next to `target_ignore_regex`.

### 7. `ignore --remove` silently drops `PATH`/`--patterns`
The clap ArgGroup allows `paths`, `patterns`, and `remove` together, but `ignore.rs:43` returns early when `remove` is present, discarding any co-issued paths/patterns without a message. Either make the group mutually exclusive or process all inputs.

### 8. `run_merge` leaks `.current_merge/` on error paths
`commands/mod.rs:282-347`: `create_dir_all` at line 284, then `fs::copy` (304), `read_zip_file`/`fs::copy` (306-309), and `resolve_merge_command` (310) all `return …?` without cleanup; only the tool-failure paths call `remove_dir_all`. A stale `.current_merge/` dir accumulates in the source repo. Clean up via a guard/defer pattern.

---

## P1 — Overcomplicated / hard-to-parse code

### 11. `pull.rs` source-path branch (`113-180`) — fall-through reassignment
`target_abs_path` is shadowed and then reassigned from inside an `if/else` where several inner branches `continue` and one falls through. Following which statements are reachable after the branch is genuinely hard. Extract a `resolve_source_to_target(...) -> Option<(target_abs, pending_task)>` function so the `continue`s become early returns.

### 12. `filepath_in_source_dir` (`lib.rs:617-646`)
Builds the source path with two regexes over filename+parent strings and string concatenation (`String::from_iter([dirname, filename])`). Same dot→`dot_prefix` mapping is reimplemented ad hoc in several commands. Iterate path components and map each leading `.` component to `dot_prefix` once.

### 13. `merge_settings` (`lib.rs:549-590`)
~40 lines of per-field `match Some/None` that could be a small `unwrap_or_else` per field (or a per-field helper). Low risk, low priority. Note `source_dir`/`target_dir` take the state values but `merge_tool_command` etc. fall back per-field — fine, just verbose.

### 14. `status.rs format_default` directory-collapse (`514-692`)
The iterative deepest-ancestor-collapse loop is ~80 lines of subtle `BTreeMap` bookkeeping. It works and is isolated, but deserves a standalone helper with a comment, or a simpler "collapse under the common ancestor that has ≥2 entries" formulation.

---

## P1 — Shrinkable / duplicated code

### 15. `main.rs` dispatch (`313-381`)
Five near-identical `if state_opt.is_none() { return Err(NotFound) } … state_opt.unwrap() … path_to_state_file.as_ref().unwrap()` blocks. Extract:

```rust
fn with_state<T>(state_opt: Option<StateObject>, path: Option<&PathBuf>,
                 f: impl FnOnce(&mut StateObject) -> Result<T, DfmError>) -> Result<T, DfmError> {
    let mut state = state_opt.ok_or_else(|| DfmError::NotFound("state file is not found".into()))?;
    let r = f(&mut state)?;
    write_state(path.unwrap(), &state)?;
    Ok(r)
}
```
Kills ~20 `unwrap()`s and the copy-paste.

### 16. "Prune matching ignore patterns after success" duplicated
`add.rs:423-430` and `pull.rs:422-429` are identical tails (`patterns_to_remove` + `prune_ignore_file`). Hoist into `commands/mod.rs`.

### 17. Per-task `if dry_run { continue; }`
Every arm of `AddTask`/`PullTask`/`InitTask`/`ForgetTask` match repeats the same guard. Guard once at the top of the loop and keep only the mutating logic in the arms.

### 18. `add.rs` symlink branch joins `current_dir` with an already-absolute path
`add.rs:93-101`: `PathBuf::from_iter(vec![current_dir, target_path.clone()])` — `target_path` comes from `list_directory` rooted at the absolute `target_dir_abs`, so it is absolute and **replaces** `current_dir` on push; the `current_dir` computation is dead. The subsequent canonicalize-parent+re-push dance is also redundant for paths already canonical-ish. Simplify or document.

### 19. Dead / unused code
- `Settings.config_file_found` (`lib.rs:444`) — set in `create_default_settings`/`merge_settings`, **never read**. Remove.
- `status.rs:135` `let _source_ignore_regex = load_ignore_regex(&source_ignore_file)?;` — loads the source ignore file and discards it; pure wasted IO. Either use it (report source-side ignores) or drop it.
- Unused direct dependencies in `Cargo.toml`: **`once_cell`**, **`aead`** (both already pulled transitively by other crates). Remove.
- `crypt.rs:48` `eprint!(": ");` always prints, even when `obtain_password_shell_command` is set — stray prompt character before command output.

### 20. `get_git_info` runs `git` twice (`status.rs:749-780`)
`rev-parse` then `status --porcelain`. Could do one `status --porcelain -b` call and parse the branch from the header line.

---

## P2 — Design patterns / robustness

- **Task-enum queue pattern** (analyze → queue enum → single execution loop) is consistent and good across add/pull/forget/init. Keep it; it's what makes the traversal/execution split readable.
- **Typed per-command args structs + `resolve_dry_run`** — good.
- **Silent error swallowing**: `main.rs:266-288` discards both `read_state` and `read_config` errors (`Err(_) => None`), so a corrupt `state.toml` or `config.toml` makes dfm silently run with defaults — destructive commands (add/pull/forget) could then act on an empty state. At minimum `warn!` the parse failure (mirroring the path-resolution warnings above).
- **Inconsistent matcher**: `check_path_matches_regex` (full-path substring, lib.rs:219) survives only for `force_encryption_for`; everything else uses component-wise matching. Either switch encryption matching to the same matcher or document why absolute-path substring is intended here.
- **`compare_files_by_timestamps` fallback clauses** (`lib.rs:961-968`) rely on `SystemTime` `==`/`<` which is coarse; the mtime-vs-sync comparisons are the classic source of spurious "inconsistent state" errors. The sha256-based `compare_files_by_content` already removed this risk for plain files; consider content comparison for encrypted files too (compare decrypted bytes vs a stored plaintext hash) so mtime granularity never breaks encryption sync.
- **`state_opt.unwrap()` + `path_to_state_file.as_ref().unwrap()`** — see #15.
- **`merge`/`run_merge`** flattens both sides into `.current_merge/` keyed only by file name; two same-named files in different dirs would collide if ever batched concurrently. Fine today (one file at a time), but worth a comment or a key suffix.

## AI / token-readability notes (what costs a model most to parse)

1. `add.rs` + `pull.rs` + `forget.rs` (~1250 lines combined) are dense loops where every branch ends in `continue`/`return` and the "which state is each binding in" question is hard to answer statically. Splitting per-scenario functions (#10, #11) would roughly halve the parse cost.
2. `pattern_matches_path_components` (`lib.rs:263-359`) is the densest pure function, but it is well-documented and has 8 focused unit tests — this is *justified* complexity; do not shrink it blind.
3. Avoid the status #5 anti-pattern (computed/colored strings in domain structs) elsewhere — it forces the reader (human or model) to mentally strip formatting when tracing data flow.
4. Repeated `PathBuf::from_iter(vec![a, b])` where `a.join(b)` reads better and is unambiguous about the replace-on-absolute semantics.

## Suggested priorities

| # | Item | Effort | Payoff |
|---|---|---|---|
| 1 | Restore permissions on decrypt (crypt.rs) | S | Security fix |
| 2 | Strip color from `StatusEntry.path`, color at render (status.rs) | S | Fixes 2 bugs + porcelain contract |
| 3 | `with_state()` helper in main.rs | S | Removes ~5x boilerplate + unwraps |
| 4 | Hoist force-encryption RegexSet (add.rs) | S | Perf |
| 5 | `.current_merge` cleanup on error (mod.rs) | S | Hygiene |
| 6 | Trim shell-command password (crypt.rs) | S | Footgun |
| 7 | Reject `..` in state keys (lib.rs) | S | Hardening |
| 8 | Refactor `remove_dots_from_path` (keep tests as spec) | M | Complexity |
| 9 | Split `forget.rs` / `pull.rs` analysis into scenario functions | M | Readability |
| 10 | Remove dead field + unused deps + unused ignore load | S | Hygiene |
| 11 | Document PBKDF2-1000 limitation / consider stronger envelope | M | Security |
| 12 | Unit tests for the changed paths (currently only `lib.rs` has them) | M | Regression safety |

Strong overall: the conflict-detection abstraction, the task-enum execution split, and the component-wise ignore matcher are well-designed. The main debt is concentrated in four spots: path string-handling in `lib.rs`, the analysis loops in `forget.rs`/`pull.rs`, color-inside-data in `status.rs`, and the duplicated dispatch in `main.rs`.
