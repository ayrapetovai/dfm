# dfm add . must handle symlinks that point to directories.
# The symlink pointer file should be created in the source, but the
# pointee directory itself must never be passed to fs::copy.

dfm init dotfiles

# Create a regular directory with a file inside
mkdir folder
write "inside folder" "folder/note.txt"

# Create a symlink to that directory
ln -s folder link_to_folder

# dfm add . must NOT fail with InvalidInput from fs::copy on a directory
dfm add .

# The regular file inside the directory should have been added
assert_source "folder/note.txt"

# The symlink pointer file should exist in source
assert_source "link_to_folder.symlink"

# Verify the pointer file content points to "folder"
assert_content_eq "$PWD/dotfiles/link_to_folder.symlink" "folder"
