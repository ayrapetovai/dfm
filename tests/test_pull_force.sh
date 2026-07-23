ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
write "$ORIGINAL" file.txt
dfm add file.txt

# modify the target file after adding
write "$MODIFIED" file.txt
assert_content_eq "file.txt" "$MODIFIED"

# pull without --force must fail because target is modified
assert_fail dfm pull file.txt

# pull with --force must succeed and overwrite target with source content
dfm pull --force file.txt

# postcondition: target file has the ORIGINAL content from source
assert_content_eq "file.txt" "$ORIGINAL"
