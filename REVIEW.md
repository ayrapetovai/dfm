
## Design patterns / robustness

- **Inconsistent matcher**: `check_path_matches_regex` (full-path substring, lib.rs:219) survives only for `force_encryption_for`; everything else uses component-wise matching. Either switch encryption matching to the same matcher or document why absolute-path substring is intended here.
- **`compare_files_by_timestamps` fallback clauses** (`lib.rs:961-968`) rely on `SystemTime` `==`/`<` which is coarse; the mtime-vs-sync comparisons are the classic source of spurious "inconsistent state" errors. The sha256-based `compare_files_by_content` already removed this risk for plain files; consider content comparison for encrypted files too (compare decrypted bytes vs a stored plaintext hash) so mtime granularity never breaks encryption sync.
- **`state_opt.unwrap()` + `path_to_state_file.as_ref().unwrap()`** — see #15.
- **`merge`/`run_merge`** flattens both sides into `.current_merge/` keyed only by file name; two same-named files in different dirs would collide if ever batched concurrently. Fine today (one file at a time), but worth a comment or a key suffix.

