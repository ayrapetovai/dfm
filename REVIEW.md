# Code Review — dfm

Review date: 2026-08-02. Scope: all of `src/` (3532 lines), `tests/launcher.sh` (213 lines), `Cargo.toml`.
Builds clean, all 12 Rust unit tests pass. The shell suite (161 tests) was not re-run.

---

## P2 — Design patterns / robustness

- **Inconsistent matcher**: `check_path_matches_regex` (full-path substring, lib.rs:219) survives only for `force_encryption_for`; everything else uses component-wise matching. Either switch encryption matching to the same matcher or document why absolute-path substring is intended here.
- **`compare_files_by_timestamps` fallback clauses** (`lib.rs:961-968`) rely on `SystemTime` `==`/`<` which is coarse; the mtime-vs-sync comparisons are the classic source of spurious "inconsistent state" errors. The sha256-based `compare_files_by_content` already removed this risk for plain files; consider content comparison for encrypted files too (compare decrypted bytes vs a stored plaintext hash) so mtime granularity never breaks encryption sync.
- **`state_opt.unwrap()` + `path_to_state_file.as_ref().unwrap()`** — see #15.
- **`merge`/`run_merge`** flattens both sides into `.current_merge/` keyed only by file name; two same-named files in different dirs would collide if ever batched concurrently. Fine today (one file at a time), but worth a comment or a key suffix.

## AI / token-readability notes (what costs a model most to parse)

1. `add.rs` + `pull.rs` + `forget.rs` (~1250 lines combined) are dense loops where every branch ends in `continue`/`return` and the "which state is each binding in" question is hard to answer statically. Splitting per-scenario functions (#10, #11) would roughly halve the parse cost.

## Suggested priorities

| # | Item | Effort | Payoff |
|---|---|---|---|
| 3 | `with_state()` helper in main.rs | S | Removes ~5x boilerplate + unwraps |
| 4 | Hoist force-encryption RegexSet (add.rs) | S | Perf |
| 6 | Trim shell-command password (crypt.rs) | S | Footgun |
| 7 | Reject `..` in state keys (lib.rs) | S | Hardening |

Strong overall: the conflict-detection abstraction, the task-enum execution split, and the component-wise ignore matcher are well-designed. The main debt is concentrated in four spots: path string-handling in `lib.rs`, the analysis loops in `forget.rs`/`pull.rs`, color-inside-data in `status.rs`, and the duplicated dispatch in `main.rs`.
