# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.


## H. Integration test review

### Strengths
- One scenario per file (`test_<description>.sh`), `set -eEu` subshells, fresh `$HOME` per
  run, stdin `< /dev/null` so password prompts can't hang the suite, grep-based assertions
  that abort on mismatch, and helper functions (`write`, `assert_*`, `add_file`,
  `assert_encrypted`) that keep tests short and readable.
- No `sleep`; timestamp-sensitive behavior is driven by mtime stamps and content hashes
  rather than wall-clock waits — good for CI determinism.
- Good coverage of conflict states (`test_status_codes.sh`), encrypted wrong-password
  retry/cache (3 tests), permission restoration, symlink pointers, and traversal pruning.
- `--porcelain` contract tested byte-exact (`^MM\tboth.txt$`).

### Gaps (each maps to a confirmed bug or risk)
| Gap | Bug/risk |
|---|---|
| `add -s` followed by `pull` / `pull <target>` on a managed symlink | A1, A2 — **critical, untested** |
| `forget --dry-run` must not change `state.toml` | A3 (`test_forget_dry_run.sh` only checks files) |
| Component containing `dot_`/`dot_prefix` round-trip | A4 |
| Valid state + deleted config file → `pull`/`add`/`status` still work | A6 |
| `-c/--config` flag actually selects another config | A7 |
| Broken symlink during `dfm add .` is skipped, not fatal | A5 |
| Non-UTF-8/binary file via `--encrypt` | F1 |
| `purge` and add/pull agree on what counts as "modified" | F2 |

### Runner issues
1. **`assert_encrypted` silently passes when `7z` is absent** (`launcher.sh:126-133`):
   `return 0` on missing `7z`. On CI (which does not install p7zip) every encryption test is a
   no-op and still reports green. Either make `7z` a CI dependency or skip encryption tests
   with a loud marker.
2. **No `cargo clippy` or `cargo fmt` in CI** (`.github/workflows/Rust.yml` only builds,
   runs `cargo test`, and `launcher.sh`) — the 80 warnings are unguarded.
3. **Test count drift**: `context.txt` says 168, actual 167.
4. **`assert_source` error message says "after 1s"** (`launcher.sh:72`) though nothing waits —
   copy-paste artifact; misleading when debugging.
5. Cleanup between tests is `find "$TMP_HOME" -mindepth 1 -exec rm -rf {} \;` — if a test
   `cd`s outside `$HOME` and writes there, files leak. Minor.
6. Global env (exported `dfm` function, `PASSWORD`) is shared across tests; a test that
   forgets to unset could leak state — current suite avoids this, but isolating env per test
   would harden it.

