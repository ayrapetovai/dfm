# sync --symlink converts an already-managed plain pair into the symlink
# layout: the target file is replaced by a symlink pointing to the source.

CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" cfg.txt
dfm add cfg.txt
assert_source "cfg.txt"
assert_fail test -L cfg.txt

# convert: regular target -> symlink over the source file
dfm sync --symlink
assert -L cfg.txt
# the symlink is a symlink and the source file still holds the content
assert_source "cfg.txt"
assert_content_eq "$PWD/dotfiles/cfg.txt" "$CONTENT"
