# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.

## C. Performance

1. **Whole-file reads in hot paths.** `compute_sha256` (`lib.rs:1038-1044`) does `fs::read`
   of the entire file; `compare_files_by_content` hashes **both** target and source on every
   `add`/`pull`/`status`. A managed 1 GB file is read twice per command. Should stream via
   `BufReader` + `io::copy` into the hasher, and could add an mtime fast-path (skip hashing
   when `mtime == sync.mtime`) since `update_sync_state` stamps both sides.
2. **Regex recompilation.** `check_path_matches_regex_component_wise` (`lib.rs:380-394`)
   first does a `RegexSet` fast check, then recompiles each pattern with `Regex::new` inside
   `pattern_matches_path_components` for every file × pattern. `RegexSet::matches()` already
   reports which patterns matched — use the matched indices instead of recompiling.
   `check_path_matches_regex_substring` (`lib.rs:233-245`) has the same issue plus an
   `unwrap()`.
3. **`file_path_relative_to`** (`lib.rs:625-648`) uses `path_components.insert(0, ...)` —
   O(depth²) per call, called everywhere. Use `ancestors().rev()` + `collect`.
4. **`status` Phase 3 stale-pattern check** (`status.rs:338-349`) is O(files × patterns);
   fine at dotfile scale but worth noting.
5. **`status` spawns `git status --porcelain -b`** (`status.rs:808-857`) on every invocation
   over the source repo — on a huge history this is the dominant cost. Must be but cached.
6. **`list_directory` collects every path in memory** (`lib.rs:889`) and `add` collects all
   tasks before executing — fine for `$HOME`, could stream for pathological trees.

