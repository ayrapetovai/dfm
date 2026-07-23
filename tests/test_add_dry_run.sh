CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt

# dry-run: no real changes should be made
dfm add -n file.txt

# postcondition: source file was NOT created
assert_no_source "file.txt"

# also test global --dry-run flag
dfm -n add file.txt
assert_no_source "file.txt"

# verify actual add still works after dry runs
dfm add file.txt
assert_source "file.txt"
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"
