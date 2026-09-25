# Road map

## Implement features

### Auto-encryption by private directories:
   when adding a file, if any directory component of its path —
   from the target-root downward to the file itself — has mode 700 or stricter
   (`mode & 0077 == 0`, group and others have no permissions),
   the file is encrypted even when it matches no `force_encryption_for` regex.
   The rule unions with `force_encryption_for` (encrypt when either applies).
   Explicit per-run encryption flags (`--encrypt` etc.) override the rule.
   It affects newly added files only:
   a plaintext file already managed under a directory that later turns private
   is left as-is (convert with explicit `--encrypt`).
   The rule is symmetric — `pull` re-evaluates it against the source-side path,
   and an already-encrypted managed file whose source-side path
   no longer satisfies the rule is decrypted in place during `pull`.

## Fix Bugs

### P1 - dfm creates directory for decryption
Reproduce: run `dfm decrypt file.encrypted`, where the file.encrypted is located
in a arbitrary directory which has nothing to do with `dotfiles`. Then we got
not only decrypted `file` to appear but also a `dotfiles` directory with temporary
directory for decryption. Fix: dfm must use $XDG_RUBTIME_DIR for decryption at first,
if no $XDG_RUBTIME_DIR is set, then find the real location of `dotfiles` directory,
and not to create one in the directory with decrypted file.

### M0 - Impossible to ignore file with mutating name
The file have name with some base64 encoded thing: `copyq_tab_JmNsaXBib2FyZA==.dat`.
It is impossible to add it to ignore list with commands:
`dfm ignore copyq_tab_.+\.dat` and as a regexp `dfm ignore -p copyq_tab_.+\.dat`.
But it is possible manually with adding a line `\.config/copyq/copyq_tab.+dat` to the ignore_file.
It must be possible to do via CLI.

### M1 — Encrypted-file diff buffers the entire plaintext in RAM
The crypt layer is deliberately streaming (peak memory `O(chunk)`), but every
diff path breaks that promise:

- `diff::diff_regular` decrypts the source with `crypt::read_encrypted_bytes`
  into a `Vec<u8>` and also reads the whole target with `fs::read` to compare.
- `diff::decrypt_source` returns a `Vec<u8>`; `diff_all` and `diff --editable`
  then write it to the scratch file (`fs::write`), so even though the scratch
  file exists, the plaintext first passes through memory.
- `run_diff` carries `Option<Vec<u8>>` alongside `write_scratch_source`.

For a multi-GiB encrypted file this is a proportional heap allocation that the
rest of the codebase explicitly avoids. The equality check can use a streaming
SHA-256 over the decrypted stream, and decryption can write straight into the
scratch file (a streaming "decrypt to path" variant of `read_encrypted_file`)
so the scratch file is filled without a full-buffer intermediate.

### M2 — Tool spawning is duplicated in `diff.rs`
`run_diff_capture` re-implements the spawn + `NotFound` mapping that
`run_tool` in `commands/mod.rs` already provides, differing only in that it
captures stdout. The strategy point "no code duplicated" is otherwise
well-respected, so this stands out. Consider a capture-capable variant
(`run_tool` with a `capture: bool`/`Stdio` parameter) so all tool handling and
its error wording live in one function. The per-path error text ("diff tool X
not found") also differs in shape from `run_tool`'s ("{label} tool X not
found").

### M3 — Missing debug entry logs in several commands
Every command logs its arguments at `debug!` on entry — except `merge`,
`config`, `paths`, `encrypt`, and `decrypt`, which start straight into their
work. This is a small asymmetry against "every step is logged with debug
level" and makes tracing `merge` more annoying than the others (merge is the
most stateful of the interactive commands). Add a one-line entry `debug!`
like the rest.

### M4 — DRY: duplicated target-dir resolution in diff.rs
`diff_regular` and `decrypt_source` each call
`calc_working_dir_paths_unchecked(settings)?.0` to get the target dir for the
inner name. Minor, but a tiny helper would remove the repetition and the two
call sites can drift.

- N1 — Stale/informal comments: `// TODO remove the symlink?` and the TODO
  chain in `pull.rs` (pointee-of-managed-symlink), the leading TODO in
  `cli.rs`, `// is ok`, and lib.rs TODOs (`read HOME depending on OS`,
  "need to make serde…") that read as unresolved decisions. Either file them
  as ROADMAP considerations or drop them.
- N2 — Import-placement inconsistency: `ignore.rs` uses the `super::` block
  *after* a top-level function (`ensure_trailing_newline`); other files group
  `use` at the top. `rustfmt` does not reorder imports, so this stays unless
  fixed by hand.
- N3 — `DFM_PROGRESS_BAR_DELAY_OFF` (a user-facing behavioural switch) is
  documented only in `context.txt` and code, not in README's config section.
- N4 — `diff_regular` recomputes the target directory twice (see M4).
- N5 — Many log strings embed `\n\t` to shape multi-line messages; consistent,
  but a helper (or losing the manual wrapping) would be cleaner in grep-able
  output.
- N6 — `tests/launcher.sh` uses GNU `readlink -f`/`find` (commit `edae29c`
  worked around a BSD incompatibility); acceptable given the Linux-only
  target, worth keeping in mind if CI ever moves.

## Considerations
- what if source file belongs to the user other than puller?
