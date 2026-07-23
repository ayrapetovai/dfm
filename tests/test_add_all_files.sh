CONTENT1="$(uuid)"
CONTENT2="$(uuid)"

dfm init dotfiles

write "$CONTENT1" file1.txt
write "$CONTENT2" file2.txt

# add all untracked files in the target directory (no paths argument)
dfm add

# postcondition: both files were copied to source
assert -f "$PWD/dotfiles/file1.txt"
assert -f "$PWD/dotfiles/file2.txt"
assert "$CONTENT1" = "$(cat "$PWD/dotfiles/file1.txt")"
assert "$CONTENT2" = "$(cat "$PWD/dotfiles/file2.txt")"
