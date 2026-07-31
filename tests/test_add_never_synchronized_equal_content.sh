CONTENT="$(uuid)"

dfm init dotfiles

# create both target and source files manually with SAME content (bypassing add)
write "$CONTENT" file.txt
mkdir -p "$PWD/dotfiles"
write "$CONTENT" "$PWD/dotfiles/file.txt"

# add without --force: content is equal → auto-record sync silently
dfm add file.txt
assert_succ grep -q 'file.txt' .local/state/dfm/state.toml

# source should still have its original content (no copy needed)
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# add the same file again — now it's tracked, should be "nothing to do"
dfm add file.txt

# source unchanged
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT"

# add with --force when content is equal: also auto-records sync
CONTENT2="$(uuid)"
write "$CONTENT2" file.txt
write "$CONTENT" "$PWD/dotfiles/file.txt"
dfm add -f file.txt

assert_succ grep -q 'file.txt' .local/state/dfm/state.toml

# source still unchanged after force
assert_content_eq "$PWD/dotfiles/file.txt" "$CONTENT2"
