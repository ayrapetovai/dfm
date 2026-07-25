CONTENT="$(uuid)"

dfm init dotfiles
mkdir -p "$PWD/.dir/shell"

write "$CONTENT" ".dir/shell/init.sh"
assert -f "$PWD/.dir/shell/init.sh"

dfm add ".dir"
assert_source "dot_dir/shell/init.sh"

rm -rf "$PWD/.dir"
dfm pull

assert -f "$PWD/.dir/shell/init.sh"
assert_content_eq "$PWD/.dir/shell/init.sh" $CONTENT
