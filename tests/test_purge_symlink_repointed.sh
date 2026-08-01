CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt
rm file.txt
dfm pull -s

# re-point the managed symlink outside the source directory
mkdir -p real_files
echo "pointee" > real_files/pointee.txt
ln -sfn "real_files/pointee.txt" file.txt

assert -L file.txt

# purge: a symlink pointing outside the source dir must be left as is
dfm purge

# postconditions: symlink still intact and still resolves to its pointee
assert -L file.txt
assert "$PWD/real_files/pointee.txt" = "$(readlink -f file.txt)"
assert_content_eq "file.txt" "pointee"

# everything else removed
assert_fail test -d "$PWD/dotfiles"
assert_fail test -d "$PWD/.config/dfm"
assert_fail test -d "$PWD/.local/state/dfm"
