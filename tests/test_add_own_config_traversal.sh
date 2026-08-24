# A bare `dfm add` (full-target traversal) picks up the dfm config file
# like any other unmanaged dotfile.

dfm init dotfiles

write "content" "file.txt"

dfm add

assert_source "file.txt"
assert_source "dot_config/dfm/config.toml"
