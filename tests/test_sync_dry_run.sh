# sync --dry-run / -n reports what it would do without changing anything.

dfm init dotfiles
write "v1" file.txt
dfm add file.txt

write "v2" file.txt

# dry-run: reports the action, changes nothing
dfm sync --dry-run
assert_content_eq "file.txt" "v2"
assert_content_eq "$PWD/dotfiles/file.txt" "v1"

# global -n also dry-runs
dfm sync -n
assert_content_eq "$PWD/dotfiles/file.txt" "v1"

# a real sync afterwards applies the change
dfm sync
assert_content_eq "$PWD/dotfiles/file.txt" "v2"
