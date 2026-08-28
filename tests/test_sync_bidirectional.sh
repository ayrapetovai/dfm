# sync copies a target-side (add-direction) change to the source.
# sync copies a source-side (pull-direction) change to the target.

dfm init dotfiles
write "v1" file.txt
dfm add file.txt

# target modified -> push to source
write "v2" file.txt
dfm sync
assert_content_eq "$PWD/dotfiles/file.txt" "v2"
assert_content_eq "file.txt" "v2"

# source modified -> pull to target
write "v3" "$PWD/dotfiles/file.txt"
dfm sync
assert_content_eq "file.txt" "v3"
assert_content_eq "$PWD/dotfiles/file.txt" "v3"
