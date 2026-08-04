dfm init dotfiles
write $(uuid) file.txt
dfm add file.txt
rm file.txt
dfm pull -s file.txt
assert -L file.txt
# pulling an already-correct managed symlink is a safe no-op: the symlink
# must be preserved, not converted to a regular copy (A2)
assert_succ dfm pull file.txt
assert -L file.txt
assert "$PWD/dotfiles/file.txt" = "$(readlink -f file.txt)"
