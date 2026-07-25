# dfm merge skips files whose target is a symlink — even if BothModified.
# Symlink-managed files don't need merging (the symlink itself is the sync mechanism).

CONTENT="$(uuid)"
SYMLINK_TARGET_CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set merge_tool_command "cp {target} {result}"

# Setup: add a file normally, then replace the target with a symlink
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# replace the managed target with a symlink pointing elsewhere
rm file.txt
write "$SYMLINK_TARGET_CONTENT" pointee
ln -s pointee file.txt

# source still exists in the cellar
assert -f "$PWD/dotfiles/file.txt"

# Act: dfm merge iterates state, finds file.txt, target is a symlink → skip
dfm merge

# Assert: target is still a symlink, source content unchanged
assert -L file.txt
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"
