# Road map

## Implement features

1. At `dfm init`, if the target-local ignore_file
   (`$XDG_STATE_HOME/dfm/ignore_file`) does not exist, create it
   and seed it with default records.
   Existing ignore_file (even an empty one) is never modified.
   For each candidate, expand `~`, canonicalize (resolve symlinks,
   component-wise) both the candidate path and the target directory,
   and if the candidate path is a prefix of the target directory path,
   add one record — the top-level component of the candidate path
   as unanchored `regex::escape` (e.g. `\.cache`, `\.local`).
   Candidates: `\.cache` ← `$XDG_CACHE_HOME`,
   `\.cargo` ← `$HOME/.cargo`, `\.npm` ← `$HOME/.npm`,
   `\.state` ← `$XDG_STATE_HOME`, `\.local` ← `$XDG_DATA_HOME`
   (covers `.local/share` too, since unanchored patterns match at any depth).

3. Rename `.dfm_ignore_file` living in source to `.dfmignore`.

4. Auto-encryption by private directories:
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

## Considerations
- what if source file belongs to the user other than puller?
