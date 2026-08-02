dfm init dotfiles

STATE_FILE="$XDG_STATE_HOME/dfm/state.toml"

write "content" file.txt
dfm add file.txt
assert_succ grep -q 'file\.txt' "$STATE_FILE"

dfm init dotfiles
assert_succ grep -q 'file\.txt' "$STATE_FILE"

write "content2" file.txt
dfm add file.txt
assert_succ grep -q "file\.txt" "$STATE_FILE"

dfm purge
dfm init dotfiles
assert_fail grep -q "file\.txt" "$STATE_FILE"
