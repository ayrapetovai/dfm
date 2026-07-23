dfm init dotfiles

# create a real file and a symlink in the target dir
mkdir -p real_files
echo "content" > "real_files/target.txt"
ln -s "real_files/target.txt" "mylink"

# add the symlink — creates a .symlink file in source
dfm add mylink
assert -f "$PWD/dotfiles/mylink.symlink"

# remove the target symlink
rm mylink
assert_fail test -f mylink

# pull by giving the source .symlink file path
# this should read the symlink file and recreate the target symlink
dfm pull "$PWD/dotfiles/mylink.symlink"

# postcondition: target symlink was recreated
assert -L mylink
assert "real_files/target.txt" = "$(readlink mylink)"
