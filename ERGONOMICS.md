# Ergonomics & UX Review

Review of dfm from the perspective of command ergonomics, feedback clarity, and overall user experience.

---

## Strengths

- **Safety-first design**: `--dry-run` on every mutating command (both per-command and global), `--force` gates all destructive paths consistently, and timestamp-based conflict detection prevents accidental overwrites.
- **Rich status output**: Colorized, categorized, auto-paged, with smart directory collapsing and many filter flags (`--conflicted`, `--modified`, `--unmanaged`, etc.). The `--porcelain` mode is a stable, script-friendly interface.
- **`paths` command**: Quick orientation without reading the config file — excellent for new users.
- **Two ignore files** (target + source) is a pragmatic design that keeps source-dir internals out of management.
- **Source/target path interchangeability**: Passing a source or target path to `pull`, `forget`, or `merge` resolves the counterpart automatically — genuinely ergonomic.

---

## Issues

### 1. `add` / `pull` naming is inconsistent

`pull` is clear (source → target), but `add` sounds like "track this file" (cf. `git add`), not "push my changes to the source directory." A `// TODO rename to push?` comment exists in the code. The confusion is most apparent with `dfm add` (no args): it traverses and pushes *all modified files*, which is very unlike `git add`.

**Recommendation**: Rename `add` → `push`. `push`/`pull` is the standard pair.

---


### 3. `--force` has overloaded meaning

In `add`/`pull`, `--force` both overrides conflict detection AND bypasses ignore patterns (removing them from the ignore file). These are two distinct operations. A user who just wants to overwrite a conflict will be surprised when their carefully curated ignore patterns vanish.

**Recommendation**: Split into `--force` (conflicts) and `--bypass-ignore` (or similar) with separate flags.

---

### 4. `config --set` stores everything as strings, no validation

All values are serialized as TOML strings regardless of the expected type. `manage_symlinks = "false"` (string) is not the same as `manage_symlinks = false` (bool). Array fields like `force_encryption_for` cannot be set at all.

**Recommendation**: Parse the value according to the field's expected type, or validate and give a clear error. Add a `--help` mode per property.

---

### 10. `calc_working_dir_paths` doesn't validate source exists

When the source directory is missing, the function returns `Ok` and subsequent operations produce confusing `NotFound` errors instead of a clear "source directory does not exist, run `dfm init`."

**Recommendation**: Validate existence early and return a user-friendly error.

---

## Minor notes

- **`purge` output** only says what was removed, not the final state.
- **No config generation command** — `init` creates one, but there's no `dfm config --defaults` to print a reference config.
- **`dfm config --set` gives no success confirmation** — it silently succeeds or fails.
- **Password cache** is only cleared on `InvalidPassword`. Entering the wrong password once caches it; the second operation (with the correct password) silently uses the wrong cached value.
- **Double negation in `forget`**: `--force` to delete a modified source means "force me to lose data," not "overcome a conflict."
- **`--managed` filter** is implemented as a hardcoded list of status codes. Adding new codes will silently exclude them from this filter.
