dfm init dotfiles

# set up two real files
mkdir -p real_files
echo "old content" > "real_files/old.txt"
echo "new content" > "real_files/new.txt"

# create symlink pointing to old.txt
ln -s "real_files/old.txt" "mylink"
dfm add mylink
assert "real_files/old.txt" = "$(cat "$PWD/dotfiles/mylink.symlink")"

# repoint the symlink to new.txt
rm mylink
ln -s "real_files/new.txt" "mylink"

# add again — should detect the pointer changed and update the symlink file
dfm add mylink

# postcondition: symlink file now points to new.txt
assert "real_files/new.txt" = "$(cat "$PWD/dotfiles/mylink.symlink")"
