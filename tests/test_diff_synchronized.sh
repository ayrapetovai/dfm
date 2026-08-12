# `dfm diff` prints "<path> is synchronized" for a managed, unmodified file —
# both when the target path and when the source path is given. An mtime-only
# change (content identical) is still "synchronized".

CONTENT="$(uuid)"

dfm init dotfiles

write "$CONTENT" file.txt
dfm add file.txt

# target path
assert_succ grep -qF "file.txt is synchronized" <<<$(dfm diff file.txt 2>/dev/null)
# source path (source-relative form)
assert_succ grep -qF "dotfiles/file.txt is synchronized" <<<$(dfm diff dotfiles/file.txt 2>/dev/null)
# absolute target path
assert_succ grep -qF "is synchronized" <<<$(dfm diff "$HOME/file.txt" 2>/dev/null)

# content unchanged but mtime changed → still synchronized
touch file.txt
assert_succ grep -qF "is synchronized" <<<$(dfm diff file.txt 2>/dev/null)

# a hidden file (dot-prefixed target ↔ dot_-prefixed source)
write "$CONTENT" .bashrc
dfm add .bashrc
assert_succ grep -qF ".bashrc is synchronized" <<<$(dfm diff .bashrc 2>/dev/null)
assert_succ grep -qF "dot_bashrc is synchronized" <<<$(dfm diff dotfiles/dot_bashrc 2>/dev/null)
