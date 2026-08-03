# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## I. Suggested priorities

1. **Fix A1/A2** (symlink integrity on pull) — data-corruption severity, no test coverage.
2. **Fix A3** (dry-run mutating state), **A4** (dot_ decode), **A6** (config-missing), **A7**
   (`-c` flag) — correctness/contract bugs.
3. **Fix A5** (broken symlink aborts `add .`) — daily-driver annoyance.
4. **Run `cargo clippy --fix` + `cargo fmt`** and add both to CI; add a p7zip CI dependency
   so encryption tests actually run.
5. **Add regression tests** for every row in the H. Gaps table.
6. **Performance pass**: stream hashing/encryption, cache compiled regexes, mtime fast-path
   in `compare_files_by_content`.
7. **Security documentation**: state explicitly that `.encrypted` archives leak paths/sizes
   and that the KDF is brute-forceable for weak passphrases; consider a stronger default.
8. **Consolidate duplicated logic** (D1-D3) and delete dead comments/attributes (E3).

