# C4 / D1 — dry-run prevents deletion
dfm init dotfiles

write "content" file.txt
dfm add file.txt

# dry-run forget — nothing should be removed
dfm forget --dry-run file.txt
assert -f "$PWD/dotfiles/file.txt"
assert -f file.txt

# actual forget removes source
dfm forget file.txt
assert_fail test -f "$PWD/dotfiles/file.txt"
assert -f file.txt
