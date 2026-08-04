# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## G. Ergonomics / predictable behavior

1. **Partial success on mid-loop failure**: `add`/`pull` run tasks after analysis; a hard
   error mid-execution aborts via `with_state` (state not persisted), leaving already-copied
   files without a sync entry → next run reports `NeverSynchronized` and demands `--force`.
   Consider reporting "N of M tasks completed" on failure.
2. **Misleading errors**: "failed to read source path from the config file" and "state
   file is not found" when it exists-but-corrupt (F5 in ./REVIEW_ROBUSTNESS.md).

