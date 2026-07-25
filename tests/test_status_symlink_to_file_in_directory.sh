CONTENT="$(uuid)"
dfm init dotfiles
mkdir dir
write "$CONTENT" dir/file.txt
ln -s dir/file.txt link-to-file.txt
assert -L link-to-file.txt
dfm status | grep -q "link-to-file.txt"
dfm add dir
assert_source "dir/file.txt"
dfm add link-to-file.txt
assert_source "link-to-file.txt.symlink"
