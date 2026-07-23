CONTENT="$(uuid)"

dfm init dotfiles

# create a real file that the symlink will point to (inside target dir, outside source dir)
mkdir -p real_files
echo "$CONTENT" > "real_files/target.txt"

# create a symlink in the target directory
ln -s "real_files/target.txt" "mylink"

# add the symlink
dfm add mylink

# source symlink file must exist and contain the pointee path
assert_source "mylink.symlink"
assert_content_eq "$PWD/dotfiles/mylink.symlink" "real_files/target.txt"
