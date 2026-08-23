# A target file without a source prints "<path> is not managed";
# a source file without a target prints "<corresponding target> is not pulled".
# Paths that exist nowhere are covered by test_diff_not_exists.sh.

dfm init dotfiles

# target path with no source
write "orphan" unmanaged.txt
RES=$(dfm diff unmanaged.txt 2>/dev/null)
assert_succ grep -qF "unmanaged.txt is not managed" <<<"$RES"
assert_fail grep -qF "does not exist" <<<"$RES"

# source path whose target does not exist
write "not-pulled" "$PWD/dotfiles/dot_bashrc"
RES=$(dfm diff dotfiles/dot_bashrc 2>/dev/null)
assert_succ grep -qF "is not pulled" <<<"$RES"

# a target path that exists in source but not in target is "not pulled" too
write "managed" managed.txt
dfm add managed.txt
rm managed.txt
RES=$(dfm diff managed.txt 2>/dev/null)
assert_succ grep -qF "managed.txt is not pulled" <<<"$RES"
