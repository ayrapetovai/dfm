dfm init dotfiles

# create a real file and a symlink pointing to it
mkdir -p real_files
echo "content" > "real_files/target.txt"
ln -s "real_files/target.txt" "mylink"

# first add creates the source symlink file
dfm add mylink
assert -f "$PWD/dotfiles/mylink.symlink"

# second add: source symlink file exists and points to the right target → skip
dfm add mylink

# postcondition: symlink file still exists with the same content
assert -f "$PWD/dotfiles/mylink.symlink"
assert "real_files/target.txt" = "$(cat "$PWD/dotfiles/mylink.symlink")"
