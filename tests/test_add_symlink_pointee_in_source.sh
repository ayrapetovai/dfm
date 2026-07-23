CONTENT="$(uuid)"

dfm init dotfiles

# create a file inside the source directory
write "$CONTENT" "$PWD/dotfiles/real_file.txt"

# create a symlink that points into the source directory
ln -s "dotfiles/real_file.txt" "mylink"

# add the symlink — its pointee is inside source dir, so it's "managed" (no symlink file)
dfm add mylink

# postcondition: no extra symlink pointer file was created
assert_no_source "mylink.symlink"
