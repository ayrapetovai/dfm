# sync updates a managed symlink's source pointer when the target symlink is
# repointed (target-side change), and leaves an unchanged symlink alone.

dfm init dotfiles
mkdir -p real_files
write "old content" "real_files/old.txt"
write "new content" "real_files/new.txt"

ln -s "real_files/old.txt" "mylink"
dfm add mylink
assert_content_eq "$PWD/dotfiles/mylink.symlink" "real_files/old.txt"

# repoint the target symlink; its source pointer now differs
rm mylink
ln -s "real_files/new.txt" "mylink"

dfm sync
assert_content_eq "$PWD/dotfiles/mylink.symlink" "real_files/new.txt"

# a second sync is a no-op (pointers agree)
dfm sync --dry-run
assert_content_eq "$PWD/dotfiles/mylink.symlink" "real_files/new.txt"
