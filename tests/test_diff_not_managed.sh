# A target file without a source prints "<path> is not managed";
# a source file without a target prints "<corresponding target> is not pulled".

dfm init dotfiles

# target path with no source
write "orphan" unmanaged.txt
assert_succ grep -qF "unmanaged.txt is not managed" <<<$(dfm diff unmanaged.txt 2>/dev/null)

# nonexistent target path with no source
assert_succ grep -qF "ghost.txt is not managed" <<<$(dfm diff ghost.txt 2>/dev/null)

# source path whose target does not exist
write "not-pulled" "$PWD/dotfiles/dot_bashrc"
assert_succ grep -qF "is not pulled" <<<$(dfm diff dotfiles/dot_bashrc 2>/dev/null)

# a target path that exists in source but not in target is "not pulled" too
write "managed" managed.txt
dfm add managed.txt
rm managed.txt
assert_succ grep -qF "managed.txt is not pulled" <<<$(dfm diff managed.txt 2>/dev/null)
