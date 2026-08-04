# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## F. Edge cases / robustness

1. **Non-UTF-8 / binary files** can't be encrypted (A/B, `crypt.rs:150`) and can't be
   content-compared (`add.rs:231-233,281-283` `read_to_string`). `read_to_string` should be
   `fs::read` for equality checks too.
2. **`purge` safety check uses mtime, add/pull use content hashes** (`purge.rs:51-97`).
   Inconsistent: restoring a file to identical content with a newer mtime makes `purge`
   demand `--force` for a non-change; conversely content-hash-based change detection in
   add/pull won't agree with purge's mtime model. Align on one detection method.
3. **`sync_file_copy` `to.parent().unwrap()`** (`mod.rs:300`) — panic risk on root-level
   destinations; prefer a handled error.
4. **`ignore.rs:56-59` `let traversed_paths = match paths { Some(p) => p, None => &vec![] }`**
   — borrows a temporary; works, but a `Vec::new()` binding is clearer.
5. **Corrupt/missing state file gives "state file is not found"** — `main.rs:270-276`
   swallows all `read_state` errors (including TOML parse errors and the tamper check) into
   `None`, so a corrupt-but-present state file surfaces as "not found". Distinguish
   `NotFound` from `InvalidData`.
6. **`get_home_path`** (`lib.rs:520-532`) is over-engineered (`envmnt::expand`); `var_os`
   suffices and avoids surprises if `$HOME` contains `$`.

