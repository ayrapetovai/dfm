# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## E. Readability, comments, and AI-assistant friendliness

1. **80 clippy warnings (25 lib / 80 bin)** — mostly `unneeded return`,
   `&PathBuf` instead of `&Path`, redundant `.write(true)` with `.append(true)`,
   `useless_vec`, redundant references in `debug!`. `cargo clippy --fix` applies 18+66 of
   them. This is the single biggest "readability for a human/AI" win available.
2. **Broken indentation**: `pull.rs:471` `for task in tasks.iter()` sits at column 0 while
   its body is indented; `add.rs:324-330` is mis-indented inside `if let Ok(re) = ...`.
   These break skimming.
3. **Dead code / dead comments** to remove:
   - `main.rs:28` `//arbitrary_command: String,`
   - `main.rs:11-15` five doc URLs (envmnt/xdg/aes-howto) unrelated to the file
   - `main.rs:62,83,109,246` `#[command(arg_required_else_help = false)]` — no-op attributes
   - `main.rs:81` `// TODO rename to push?`
   - `lib.rs:466,495` `// pub compare_content: Option<bool>,`
   - `status.rs:757` `_force_pager` unused parameter
4. **Typo**: `main.rs:34` "quite" → "quiet".
5. **Misleading comment in `context.txt`**: "Paths built with `Path::join` (never
   string-concat)" — but `lib.rs:160,165,401,543` build names with `format!("{}/{}", ...)`.
6. **Stale docs**:
   - `context.txt` says **168** tests; actual count is **167**.
   - `README.md §2.1 step 3` says `init` pulls config from inside the source dir; the code
     explicitly does not (`init.rs`, confirmed in `context.txt:147`).
   - `README.md:24` `git -c "initial"` is not a valid command (should be
     `git commit -m "initial"`).
7. **Verbose clap boilerplate**: every bool flag repeats `num_args = 0, default_value_t =
   false` (main.rs throughout) — redundant for booleans and inflates the token count of the
   most-read file. Grouping/collapsing would help.
8. **Comment-to-code ratio is good** (doc comments on non-trivial functions are useful:
   `ProgressLine`, `pattern_matches_path_components`, `with_state`). Keep those; cut the
   dead ones above.

