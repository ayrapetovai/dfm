# Road map

## Implement features

1. Auto-encryption by private directories:
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
