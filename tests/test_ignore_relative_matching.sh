# Ignore matching is relative to the target directory root: `dfm ignore abc`
# ignores only the top-level ./abc/, never ./dir/abc/ of another directory.
#
# Layout (the reported bug case):
#   ./abc/file1
#   ./dir/file2
#   ./dir/abc/file3

dfm init dotfiles

write "f1" "abc/file1"
write "f2" "dir/file2"
write "f3" "dir/abc/file3"

dfm ignore abc

# The top-level abc/ is ignored: pruned from the walk, shown as one entry
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "!!  abc/" <<<"$RES"

# The nested dir/abc/ is NOT ignored — its file is visible as unmanaged,
# and the directory itself never appears as an ignored entry
RES=$(dfm status --short 2>/dev/null)
assert_succ grep -qF "?? dir/abc/file3" <<<"$RES"
assert_fail grep -qF "!! dir/abc" <<<"$RES"
# dir/ is not ignored and hase several objects, so it is folded
RES=$(dfm status 2>/dev/null)
assert_succ grep -qF "dir/*" <<<"$RES"
assert_fail grep -qF "dir/file2" <<<"$RES"

# add manages dir/file2 and dir/abc/file3, but skips ./abc
dfm add
assert_no_source "abc/file1"
assert_source "dir/file2"
assert_source "dir/abc/file3"
