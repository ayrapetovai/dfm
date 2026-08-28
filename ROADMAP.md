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

2. 'status' subcommand gets filter flag `--encrypted` (`-e`):
   it shows the encrypted set — entries whose `.encrypted` file
   in the source dir has a managed plaintext counterpart (a synchronized pair).
   `-e` overrides all other filter flags (`--modified`, `--ignored`, etc.)
   and always reports the encrypted entries in every category;
   paths are listed relative to the target directory like other commands.
   `--porcelain` / `--short` formats are unchanged —
   the flag only filters which entries appear.
   Orphaned `.encrypted` files (no managed plaintext source) are never shown.
   This is the single definition of `status --encrypted`
   (formerly described by both item 2 and item 10).

3. In default human status output, any entry that corresponds to an encrypted
   source (regardless of state: synchronized, modified, unpulled, unmanaged)
   is marked with the word `(encrypted)` at the end of its line,
   right-aligned so all markers in a block line up in one column.
   Grouped directory entries (printed as `dir/*`) receive `(encrypted)`
   after the star when the whole group is encrypted: `.ssh/* (encrypted)`.
   `--short` and `--porcelain` output are unchanged.

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

5. 'status' accepts any combination of its flags in any order,
   each in short or long spelling interchangeably
   (both `-e`/`--encrypted`, `-m`/`--modified`, `-s`/`--short`, etc.).
   Combining flags is additive: every matching block is printed —
   e.g. `--managed --unmanaged` shows both blocks;
   there is no "contradictory flags" error
   and no hidden priority conflict
   (the only per-flag override is `-e` overrides other filters,
   per the `status --encrypted` item).

6. `diff --editable` (`-e`):
   before anything else dfm verifies the target and source files
   have equal content;
   if they differ it removes any scratch copies,
   prints an error and returns non-zero without doing any work.
   When the contents are equal, writable temp copies are made
   and the diff tool runs against them (the user edits them).
   If the tool exits 0, the edited buffers are written back
   over both the target and the source files
   and the recorded sync timestamp is updated
   (contents matched, edit applied cleanly).
   If the tool exits non-zero, the edits are discarded
   (copies removed, nothing written back, sync untouched).
   When the edited side is an encrypted source,
   the written-back content is encrypted again.
   Note: `-e` is also the status `--encrypted` short flag —
   same letter on different subcommands, so it is legal.

7. Add 'sync' subcommand implemented.

8. Add a progress bar for the action phase of the `add`, `pull` and `sync`
   commands (the walkdir phase already has progress).
   One step = one processed file
   (each planned task advances the counter;
   the total is the number of planned tasks).
   The bar is printed to stdout and only when stdout is a TTY —
   it is suppressed when output is redirected or piped,
   and it is enabled by default regardless of `-v`/`-n`.

9. When `dfm status <path>` is given exactly one PATH argument
   and there is nothing else to report,
   print `<path> is up-to-date.` — the path as typed (file or directory) —
   to stdout, instead of `All up-to-date.`.
   Multiple path arguments keep the old `All up-to-date.` message.
   `--porcelain`/`--short` output is unchanged.

10. Add flag --encrypted (-e) for subcommand 'status', when given the dfm must
    print a block of filenames relative to target path (like other commands)
    that are encrypted in source directory.
    — DUPLICATE of item 2, merged there; delete this bullet when implementing.

11. Add `dfm diff --all` (`-a`):
    diffs all modified files and prints the output to stdout
    through the same pager mechanism the `status` command uses.
    `--all` is true by default, controlled by the config bool
    `diff_all_default` (default true);
    the command template is the config string `diff_all_tool_command`.
    Default template: `diff -u --color=always {source} {target}`
    for TargetModified
    and `diff -u --color=always {target} {source}` for SourceModified
    (the `--color-always` spelling in the draft is a typo).
    Each assembled command is executed as a single argument to a shell —
    `sh -c '<filled template>'` — with the `{source}`/`{target}`
    placeholders substituted verbatim (paths with spaces or quotes
    require a custom template).
    BothModified files also get a diff printed;
    never-synchronized files produce nothing.
    Encrypted sources are decrypted to a scratch copy
    and diffed against the plaintext side (requires the password).
    Explicit path arguments restrict the diff to those files;
    `--all` is still the default.

12. Command 'dfm status' without flags must not print block Unpulled implemented.

## Documents

- License: switch to GPL-3.0-only, applying to all files in the repository.
  The license text is NOT embedded into files —
  it lives in a single separate `LICENSE` file at the repository root.
  `Cargo.toml` `license` and packaging (AUR) metadata
  are updated in the same commit.

- In README.md's general description, state that dfm works only locally:
  the sync is purely local —
  dfm only ever moves files between local directories on the same machine,
  with no remote/host/server mode;
  it never talks to git servers, the network, cloud, or any external service.
  This is additional to (not a replacement for)
  the existing "git commands are not embedded" limitation.

- Shrink README.md and context.txt: define a per-section max-size budget
  (in lines) for each file, then trim each section
  so that both files fit within their budgets.
  The budgets are agreed before trimming,
  and fitting them is the verifiable outcome.

## Considerations
- what if source file belongs to the user other than puller?
