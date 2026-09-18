# A directory path to `dfm diff` batch-diffs every modified file inside it
# (`--all` has the same effect and additionally turns plain file paths into
# batch scopes). Files outside the scope, up-to-date files and never-synced
# files produce nothing; `--all` on a missing or out-of-scope path is an error.

dfm init dotfiles

# Target-modified under the scope dir -> template `{source} {target}`.
write "orig" .sub/alpha.txt
dfm add .sub/alpha.txt
write "NEWTARGET" .sub/alpha.txt

# Source-modified under the scope dir -> template `{target} {source}`.
write "orig" .sub/beta.txt
dfm add .sub/beta.txt
write "NEWSOURCE" "$PWD/dotfiles/dot_sub/beta.txt"

# Up-to-date under the scope dir -> silent.
write "same" .sub/gamma.txt
dfm add .sub/gamma.txt

# Never-synchronized (both sides present, no state record) -> silent.
write "aaa" .sub/ns.txt
write "bbb" "$PWD/dotfiles/dot_sub/ns.txt"

# Modified file outside the scope dir -> not shown.
write "orig" top.txt
dfm add top.txt
write "TOP" top.txt

# A directory path diffs every modified file in it, without needing `--all`.
OUT=$(dfm diff .sub 2>/dev/null)
assert_succ grep -qF -- "--- $PWD/dotfiles/dot_sub/alpha.txt" <<<"$OUT"
assert_succ grep -qF -- "+++ $PWD/.sub/alpha.txt" <<<"$OUT"
assert_succ grep -qF -- "--- $PWD/.sub/beta.txt" <<<"$OUT"
assert_succ grep -qF -- "+++ $PWD/dotfiles/dot_sub/beta.txt" <<<"$OUT"
assert_fail grep -qF "top.txt" <<<"$OUT"
assert_fail grep -qF "gamma.txt" <<<"$OUT"
assert_fail grep -qF "ns.txt" <<<"$OUT"

# `--all` on the same directory is identical.
OUT2=$(dfm diff -a .sub 2>/dev/null)
assert_succ grep -qF -- "--- $PWD/dotfiles/dot_sub/alpha.txt" <<<"$OUT2"
assert_fail grep -qF "top.txt" <<<"$OUT2"

# A source-side directory path maps back to the target scope.
OUT3=$(dfm diff dotfiles/dot_sub 2>/dev/null)
assert_succ grep -qF -- "--- $PWD/dotfiles/dot_sub/alpha.txt" <<<"$OUT3"
assert_fail grep -qF "top.txt" <<<"$OUT3"

# `--all` turns a plain file path into a batch scope as well.
OUT4=$(dfm diff -a top.txt 2>/dev/null)
assert_succ grep -qF -- "+++ $PWD/top.txt" <<<"$OUT4"
assert_fail grep -qF "alpha.txt" <<<"$OUT4"

# `--all` with no modified files in the directory produces nothing.
mkdir -p quietdir
write "quiet" quietdir/q.txt
dfm add quietdir/q.txt
OUT5=$(dfm diff -a quietdir 2>/dev/null)
assert -z "$OUT5"

# `--all` on a missing path is an error, like `status <path>`.
run_fail dfm diff -a no-such-dir
assert_succ grep -qF "path does not exist" <<<"$FAIL_OUTPUT"

# `--all` on a path outside the target/source dirs is an error.
run_fail dfm diff -a /tmp/outside-the-target
assert_succ grep -qF "outside the target directory" <<<"$FAIL_OUTPUT"

# No scratch dir is left behind.
assert ! -e "$PWD/dotfiles/.current_diff"

