# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## G. Ergonomics / predictable behavior

1. **Global flags must precede the subcommand**: `dfm pull -v 2` fails with "unexpected
   argument '-v'"; only `dfm -v 2 pull` works. Common user expectation is the reverse. Either
   propagate global flags into subcommands or document prominently.
2. **`-c/--config` does nothing** (A7) — worst kind of ergonomics bug: the flag appears to
   work.
3. **Partial success on mid-loop failure**: `add`/`pull` run tasks after analysis; a hard
   error mid-execution aborts via `with_state` (state not persisted), leaving already-copied
   files without a sync entry → next run reports `NeverSynchronized` and demands `--force`.
   Consider reporting "N of M tasks completed" on failure.
4. **Misleading errors**: "failed to read source path from the config file" (A6) and "state
   file is not found" when it exists-but-corrupt (F5).
5. **`init` recreating nothing / `init` idempotency** is decent; `init` on an existing state
   leaves syncs intact (good).
6. Good: explicit `--force` gating on every destructive path, `--dry-run` honored
   everywhere *except* forget's state write (A3), task descriptions printed before
   execution and under dry-run.
