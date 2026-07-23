# B3a1iα — forget by source symlink file path, target symlink exists and matches
dfm init dotfiles

mkdir -p real_files
echo "real content" > "real_files/other.txt"
ln -s "real_files/other.txt" "mylink"

dfm add mylink
assert_source "mylink.symlink"
assert -L mylink

# forget by providing the source symlink file path
dfm forget "$PWD/dotfiles/mylink.symlink"

# source symlink file removed
assert_no_source "mylink.symlink"
# target symlink still exists (pointee outside source dir)
assert -L mylink
