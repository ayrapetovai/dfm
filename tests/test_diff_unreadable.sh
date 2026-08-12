# An unreadable (permission-denied) target is skipped with a warning, like in
# the other commands, and does not fail `dfm diff`.

CONTENT="$(uuid)"

dfm init dotfiles
dfm config --set diff_tool_command "diff -u {target} {source}"

write "$CONTENT" locked.txt
dfm add locked.txt
chmod 000 locked.txt

dfm diff locked.txt >out.txt 2>&1
assert_succ grep -qF "skipping unreadable path" out.txt

chmod 644 locked.txt
assert_succ grep -qF "is synchronized" <<<$(dfm diff locked.txt 2>/dev/null)
