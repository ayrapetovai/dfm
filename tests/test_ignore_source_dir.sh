# D2 — ignore a file in the source directory
dfm init dotfiles

echo "content" > file.txt
dfm add file.txt
assert -f "$PWD/dotfiles/file.txt"

# count lines in source ignore file before
BEFORE=$(wc -l < "$PWD/dotfiles/.dfm_ignore_file")

# ignore the file by its source dir path
dfm ignore "$PWD/dotfiles/file.txt"

# source ignore file should have gained a new line
AFTER=$(wc -l < "$PWD/dotfiles/.dfm_ignore_file")
assert "$BEFORE" -lt "$AFTER"
