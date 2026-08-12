# A missing diff tool must fail `dfm diff` with exit code 1.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set diff_tool_command "this-program-does-not-exist {target} {source}"

write "$CONTENT" file.txt
dfm add file.txt
write "$MODIFIED" file.txt

assert_fail dfm diff file.txt 2>/dev/null
assert_fail dfm diff dotfiles/file.txt 2>/dev/null
