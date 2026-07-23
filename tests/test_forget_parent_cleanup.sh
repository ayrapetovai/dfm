# D3 — forget removes empty parent directories in the source dir
dfm init dotfiles

# create a nested file structure
mkdir -p subdir/nested
echo "content" > subdir/nested/file.txt
dfm add subdir/nested/file.txt

# verify source dir has the nested structure
assert -f "$PWD/dotfiles/subdir/nested/file.txt"

# forget the file
dfm forget subdir/nested/file.txt

# source file removed
assert_fail test -f "$PWD/dotfiles/subdir/nested/file.txt"

# empty parent dirs should also be cleaned up
assert_fail test -d "$PWD/dotfiles/subdir/nested"
assert_fail test -d "$PWD/dotfiles/subdir"
