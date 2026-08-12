# `dfm diff` on a symlink prints the target's pointee and the source's
# pointee (from the `.symlink` pointer file) and never runs the diff tool.

dfm init dotfiles

# `add -s`: target replaced by a symlink into the source dir
CONTENT="$(uuid)"
write "$CONTENT" link.txt
dfm add -s link.txt
dfm diff link.txt >out.txt 2>/dev/null
assert_succ grep -qF "link.txt is a symlink pointing to" out.txt
assert_succ grep -qF "dotfiles/link.txt" out.txt

# `add` on an existing symlink: a `.symlink` pointer file is written
write "pointee" target_file
ln -s target_file mylink
dfm add mylink >/dev/null 2>&1
assert_source "mylink.symlink"

dfm diff mylink >out.txt 2>/dev/null
assert_succ grep -qF "mylink is a symlink pointing to target_file" out.txt
assert_succ grep -qF "points to target_file" out.txt

# the source-side path (the pointer file) prints the same information
dfm diff dotfiles/mylink.symlink >out.txt 2>/dev/null
assert_succ grep -qF "is a symlink pointing to target_file" out.txt
assert_succ grep -qF "points to target_file" out.txt
