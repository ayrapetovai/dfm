# Code Review — dfm

Review date: 2026-08-01

This document records a code review of the dfm source tree (`src/`). It lists
bad patterns found and proposes replacement patterns that are clearer for AI
analysis (explicit, testable, and free of hidden global state).

## A. Bad patterns found

### A1. Global, env-bound `static` that can panic — `src/lib.rs:135`

```rust
static XDG: Lazy<Xdg> = Lazy::new(|| Xdg::new().expect("XDG directories must be available"));
```

- Reads `HOME`/`USER` from the process env at first touch, and **panics** if both
  are unset.
- Every path resolver (`calc_state_file_path`, `calc_config_file_path`,
  `calc_local_ignore_file`, ...) silently depends on this global. Nothing can be
  injected, which is exactly why a test could not sandbox its paths and
  destroyed the real `~/.local/state/dfm` (see `tests/test_purge_unresolvable_env.sh`).

### A2. `&Args` drilled into every command + `unreachable code reached` boilerplate

Every command starts with (e.g. `add.rs:17-25`):

```rust
let Command::Add { .. } = &args.command else {
    return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `add`", args.command)));
};
```

Repeated 10 times. It is dead defensive code — the dispatcher already matched
the variant. It also forces `*force`, `args.dry_run` re-reading and makes every
command signature `(settings, &Args, ...)`.

### A4. `.unwrap()` / `to_str().unwrap()` on user-controlled paths

Non-UTF-8 paths panic. `to_str().unwrap()` appears ~30 times;
`PathBuf::from_iter([source_dir_abs_path.to_str().unwrap(), ...])` builds paths
by string-concatenation (`lib.rs:694`, `status.rs:73`, `forget.rs:271`).
`context.txt` even admits this ("many `to_str().unwrap()`").

### A5. Ignore-file edit logic copy-pasted 3 times

The "remove `--force`d patterns from ignore file" filter appears in
`add.rs:436-449` and `pull.rs:417-431`; record removal in `ignore.rs:200-244`.
Three near-identical read/filter/write blocks — they have already started to
drift.

### A7. Subtle behavioral divergence hidden in a flag

`add.rs:65` `is_dir_traversal` switches a conflict from "error" to "silent skip"
depending on whether the user named the path or traversed. The invariant is real
but invisible; the same dual-mode logic is scattered across several
`if !is_dir_traversal` branches.

### A8. Magic strings

Status codes (`"MM"`, `"M "`, `"!?"`, `"LL"`) as `&'static str` throughout
`status.rs`; the pull dotfile filter regex `r#"^(.+/)?[^.][^/]+$"#`
(`pull.rs:83`) is an unlabeled constant; `"."`/`"x"` sentinels in
`is_dir_ignored`.

### Minor

- Both `lazy_static` **and** `once_cell::sync::Lazy` imported; two global-cache
  styles (`PASSWORD_CACHE` Mutex vs `XDG` Lazy).

## B. Patterns clearer for AI analysis

1. **Inject a context struct instead of globals.** `struct Ctx { paths: Paths,
   settings: Settings, state: ... }` with `Paths` built once from an explicit
   `Xdg::with_home(...)` (or injected home). Path resolution becomes pure
   functions on `&Paths`; commands and tests call the same code with a fake
   home. This kills A1 and the panic hazard, and would have prevented the purge
   bug by construction.

2. **Typed per-command args; delete the dispatcher boilerplate.** Change each
   command to accept a small struct (`AddArgs { paths, force, symlink, encrypt,
   dry_run }`) rather than `&Args`, and let `main` map the clap variant into it.
   Removes A2 entirely.

3. **Separate *plan* from *execute* as reusable functions.** The task-enum idea
   is right; make it explicit and pure: `fn plan_add(ctx, ...) ->
   Result<PlannedAdd>` where the analysis loop touches **no filesystem writes**,
   then `fn execute_add(planned, ctx)` runs tasks and updates state. The analysis
   becomes unit-testable without a sandbox, and the "silent-skip vs conflict"
   decision (A7) becomes one small pure decision function returning
   `Action::{Skip, Conflict, Task(_)}`.

4. **One decision table, not two matching functions.** Rename to state behavior:
   `matches_substring` (encryption) vs `matches_component_wise` (ignore).

5. **Newtypes + typed errors over `PathBuf` + `String` keys.** `type StateKey =
   String` (with a `from_source_rel` constructor), `StatusCode` enum with
   `Display`, and a `fn rel_str(p: &Path) -> Result<&str, DfmError>` wrapper to
   replace `to_str().unwrap()`. This removes A4 and A8.

6. **One ignore-file edit helper.** `fn edit_ignore_file(path, &dyn
   Fn(&mut Vec<String>)) -> Result<()>` shared by add/pull/ignore — kills A5 and
   its drift.

7. **Keep a single doc of record.** Pick `context.txt` (it is the accurate one),
   delete dead config fields, and have the README reference it instead of
   restating (and contradicting) behavior. Reduces A10.
