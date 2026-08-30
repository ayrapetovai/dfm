# `dfm diff` (no paths) = `--all`: diffs every modified managed file in one
# batch, using the non-interactive default templates and correct argument
# order. BothModified uses the target template; up-to-date, unmanaged and
# never-synced files produce nothing; encrypted sources are decrypted and
# diffed. Explicit paths keep the per-path interactive behavior.

PASSWORD="$(uuid)"

dfm init dotfiles

# Target-modified -> template `{source} {target}`: `--- source`, `+++ target`.
write "orig" a.txt
dfm add a.txt
write "NEWTARGET" a.txt

# Source-modified -> template `{target} {source}`: `--- target`, `+++ source`.
write "orig" b.txt
dfm add b.txt
write "NEWSOURCE" "$PWD/dotfiles/b.txt"

# Both-modified -> target template `{source} {target}`.
write "orig" c.txt
dfm add c.txt
write "CT" c.txt
write "CS" "$PWD/dotfiles/c.txt"

# Up-to-date -> silent.
write "same" d.txt
dfm add d.txt

# Never-synchronized (both sides present, no state record) -> silent.
mkdir -p nsdir
write "aaa" nsdir/ns.txt
write "bbb" "$PWD/dotfiles/nsdir/ns.txt"

# Encrypted, target-modified -> decrypted source is diffed against the target.
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
write "secret" secret.txt
dfm add --encrypt secret.txt
write "CHANGED" secret.txt

OUT=$(dfm diff 2>/dev/null)
assert -s <<<"$OUT"

# All three modified files present, with the correct header ordering.
assert_succ grep -qF -- "--- $PWD/dotfiles/a.txt" <<<"$OUT"
assert_succ grep -qF -- "+++ $PWD/a.txt" <<<"$OUT"
assert_succ grep -qF -- "--- $PWD/b.txt" <<<"$OUT"
assert_succ grep -qF -- "+++ $PWD/dotfiles/b.txt" <<<"$OUT"
# BothModified -> target template direction (source header then target output).
assert_succ grep -qF -- "--- $PWD/dotfiles/c.txt" <<<"$OUT"
assert_succ grep -qF -- "+++ $PWD/c.txt" <<<"$OUT"

# Up-to-date and never-synced files produce no diff lines.
assert_fail grep -qF "nsdir" <<<"$OUT"
assert_fail grep -qF "d.txt" <<<"$OUT"

# The encrypted source was decrypted and diffed.
assert_succ grep -qF "CHANGED" <<<"$OUT"
assert_succ grep -qF "secret" <<<"$OUT"

# A `.current_diff` scratch dir must not be left behind.
assert ! -e "$PWD/dotfiles/.current_diff"

# `-a` behaves the same as the no-arg default.
OUT2=$(dfm diff -a 2>/dev/null)
assert -s <<<"$OUT2"

# Explicit paths keep the per-path interactive mode (custom diff_tool_command,
# not the batch templates) — a path diffs only that file.
dfm config --set diff_tool_command "diff -u {target} {source}"
write "P" p.txt
dfm add p.txt
write "PM" p.txt
OUT3=$(dfm diff p.txt 2>/dev/null)
assert_succ grep -qF -- "--- $PWD/p.txt" <<<"$OUT3"
# no other file appears in the per-path diff
assert_fail grep -qF "a.txt" <<<"$OUT3"
