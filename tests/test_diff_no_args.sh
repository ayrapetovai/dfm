# `dfm diff` with no paths is the `--all` batch mode: it diffs every modified
# file. With nothing to diff it produces no output and exits 0; a modified
# managed file appears in the concatenated diff. The state file is required,
# so an uninitialized run is an error.

# uninitialized -> batch mode needs the state file, so it must fail
run_fail dfm diff

dfm init dotfiles

# nothing modified -> empty output, exit 0
assert_succ dfm diff >out.txt 2>/dev/null
assert ! -s out.txt

# a managed, modified file shows up in the batch diff (default templates)
CONTENT="$(uuid)"
MODIFIED="$(uuid)"
write "$CONTENT" file.txt
dfm add file.txt
write "$MODIFIED" file.txt

dfm diff >diff.out 2>/dev/null
assert -s diff.out
assert_succ grep -qF "$CONTENT" diff.out
assert_succ grep -qF "$MODIFIED" diff.out
