CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt
rm file.txt

# dry-run: should not restore the target file
dfm pull -n
assert_fail test -f file.txt

# global --dry-run flag should also prevent changes
dfm -n pull
assert_fail test -f file.txt

# verify actual pull still works
dfm pull
assert -f file.txt
assert "$CONTENT" = "$(cat file.txt)"
