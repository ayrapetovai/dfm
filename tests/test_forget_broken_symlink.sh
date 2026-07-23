# B2a — forget a broken symlink (pointee doesn't exist)
dfm init dotfiles

# create a broken symlink in the target directory
ln -s /nonexistent/pointee broken_link

# forget the broken symlink — should succeed (nothing managed to remove)
dfm forget broken_link

# broken symlink should still exist (it's not managed)
assert -L broken_link
