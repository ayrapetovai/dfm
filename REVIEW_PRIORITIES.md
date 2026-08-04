# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## I. Suggested priorities

1. **Run `cargo clippy --fix` + `cargo fmt`** and add both to CI; add a p7zip CI dependency
   so encryption tests actually run.
2. **Add regression tests** for every row in the H. Gaps table.
3. **Performance pass**: stream hashing/encryption, cache compiled regexes, mtime fast-path
   in `compare_files_by_content`.
4. **Security documentation**: state explicitly that `.encrypted` archives leak paths/sizes
   and that the KDF is brute-forceable for weak passphrases; consider a stronger default.
5. **Consolidate duplicated logic** (D1-D3) and delete dead comments/attributes (E3).

