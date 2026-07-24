# C4 / D1 — dry-run prevents deletion
dfm init dotfiles

write "content" file.txt
dfm add file.txt

# dry-run forget — nothing should be removed
dfm forget --dry-run file.txt
assert_source "file.txt"
assert -f file.txt

# dry-run + force — dry-run must still win
dfm forget --dry-run --force file.txt
assert_source "file.txt"
assert -f file.txt

# actual forget removes source
dfm forget file.txt
assert_no_source "file.txt"
assert -f file.txt
