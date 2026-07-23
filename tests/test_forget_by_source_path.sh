# B2b — forget by source path (target file deleted, source still exists)
dfm init dotfiles

write "content" file.txt
dfm add file.txt

# delete target file so canonicalize fails
rm file.txt

# forget by the relative target path — the code finds the source file
dfm forget file.txt

# source file must be removed
assert_no_source "file.txt"
