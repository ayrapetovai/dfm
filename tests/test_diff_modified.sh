# A modified file makes `dfm diff` run the configured diff tool (fork-exec,
# no shell) with {target} and {source} substituted. The diff tool's own exit
# status must not fail dfm (`diff` exits 1 when files differ), and diff must
# not modify any file.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set diff_tool_command "diff -u {target} {source}"

write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# modify only the target
write "$MODIFIED" file.txt

dfm diff file.txt >diff.out 2>/dev/null
assert_succ grep -qF -- "-$MODIFIED" diff.out
assert_succ grep -qF -- "+$CONTENT" diff.out

# same via the source path
dfm diff dotfiles/file.txt >diff.out 2>/dev/null
assert_succ grep -qF -- "-$MODIFIED" diff.out

# diff must not modify either side
assert_content_eq "file.txt" "$MODIFIED"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# files that differ but were never synchronized still show a diff
write "aaa" ns.txt
write "bbb" "$PWD/dotfiles/ns.txt"
dfm diff ns.txt >diff.out 2>/dev/null
assert_succ grep -qF -- "-aaa" diff.out
assert_succ grep -qF -- "+bbb" diff.out

# ... but identical contents are reported as synchronized even without a sync
write "same" eq.txt
write "same" "$PWD/dotfiles/eq.txt"
assert_succ grep -qF "eq.txt is synchronized" <<<$(dfm diff eq.txt 2>/dev/null)
