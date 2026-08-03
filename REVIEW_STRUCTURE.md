# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## D. Structure & "common logic must not be distributed"

1. **The "rel → remove_dots → to_string_lossy" state-key idiom is duplicated ~10×** in:
   `mod.rs` (`get_sync_time`, `remove_sync_state`, `update_sync_state`), `forget.rs`
   (`source_to_state_key`), `status.rs`, `merge.rs`, `purge.rs`. Extract one helper
   `state_key_for(abs_path, root) -> String`.
2. **The "ignored → if force remove-pattern else skip" block is duplicated 4×**:
   `add.rs:180-188`, `add.rs:90-93`, `pull.rs:115-123`, `pull.rs:185-193`. Extract a
   `handle_ignore_or_override(...) -> Option<pattern>` helper.
3. **Conflict `CompareByTimestamp` match arms are duplicated** across `add.rs:253-296`,
   `pull.rs:281-308`, `forget.rs:179-215`, `merge.rs:160-171` with subtly different
   semantics per command. Consolidation is riskier (each command intentionally differs), but
   the four "if !force { warn + return } else { queue }" arms could share a helper.
4. **Trivial wrapper functions**: `calc_source_ignore_file` (`lib.rs:183-186`) is just
   `Ok(dir.join(IGNORE_FILE_NAME_IN_SOURCE_DIR))`; `open_or_create_target_ignore_file`
   duplicates `open_or_create_file` logic.
5. **`source_rel_to_target_rel` vs the encoder** live in different modules (`mod.rs` vs
   `lib.rs`) and are asymmetric (see A4) — they should be co-located and mirrored.
6. **`with_state` vs `with_state_even_if_error`** (`main.rs:377-399`) is a good, documented
   pattern — but the forget dry-run bug (A3) shows the risk: persistence runs even on
   `--dry-run`. The dry-run flag must gate *all* mutations, including state.
7. **`resolve_source_variant`/`get_sync_time`/`source_rel_to_target_rel` in `mod.rs` vs
   `filepath_in_source_dir` in `lib.rs`** — the split is documented in `context.txt`, but the
   two path-mapping families belong together.

