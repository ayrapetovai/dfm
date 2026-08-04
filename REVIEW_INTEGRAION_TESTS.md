# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.


## H. Integration test review

### Gaps (each maps to a confirmed bug or risk)
| Gap | Bug/risk |
|---|---|
| Non-UTF-8/binary file via `--encrypt` | F1 |
| `purge` and add/pull agree on what counts as "modified" | F2 |

### Runner issues
1. **`assert_source` error message says "after 1s"** (`launcher.sh:72`) though nothing waits —
   copy-paste artifact; misleading when debugging.
2. Cleanup between tests is `find "$TMP_HOME" -mindepth 1 -exec rm -rf {} \;` — if a test
   `cd`s outside `$HOME` and writes there, files leak. Minor.
3. Global env (exported `dfm` function, `PASSWORD`) is shared across tests; a test that
   forgets to unset could leak state — current suite avoids this, but isolating env per test
   would harden it.

