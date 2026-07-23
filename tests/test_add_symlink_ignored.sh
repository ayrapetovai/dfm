dfm init dotfiles

# create a symlink
mkdir -p real_files
echo "content" > "real_files/target.txt"
ln -s "real_files/target.txt" "mylink"

# ignore the symlink by path
dfm ignore mylink

# add should skip the ignored symlink
dfm add mylink

# postcondition: no symlink file was created in source
assert_fail test -f "$PWD/dotfiles/mylink.symlink"
