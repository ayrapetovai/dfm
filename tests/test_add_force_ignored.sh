# add --force adds an ignored file and removes the ignore pattern

CONTENT="$(uuid)"

dfm init dotfiles

# create a file and ignore it before adding
write "$CONTENT" target_file.txt
dfm ignore target_file.txt

# default add — must skip (file is ignored)
dfm add target_file.txt 2>/dev/null
assert_no_source target_file.txt

# add --force — must add the file and remove the ignore pattern
dfm add --force target_file.txt 2>/dev/null
assert_source target_file.txt
assert_content_eq "$PWD/dotfiles/target_file.txt" "$CONTENT"

# the ignore pattern must be gone — re-adding without --force should work
rm "$PWD/dotfiles/target_file.txt"
dfm add target_file.txt 2>/dev/null
assert_source target_file.txt
