# init's .dfm_root pointer chain must only accept a plain directory name or
# "." — values that escape the source directory ("..", absolute paths, nested
# paths, empty) must be rejected instead of being followed.

mkdir -p dotfiles

# `..` in a pointer would walk up out of the source directory
echo ".." >dotfiles/.dfm_root
assert_fail dfm init dotfiles
assert_fail test -f "$PWD/.config/dfm/config.toml"

# absolute path pointer must be rejected
printf '/etc' >dotfiles/.dfm_root
assert_fail dfm init dotfiles
assert_fail test -f "$PWD/.config/dfm/config.toml"

# a nested/multi-component pointer must be rejected
printf 'a/b' >dotfiles/.dfm_root
assert_fail dfm init dotfiles

# an empty pointer must be rejected
: >dotfiles/.dfm_root
assert_fail dfm init dotfiles

# a trailing-slash pointer variant must be rejected
printf 'sub/' >dotfiles/.dfm_root
assert_fail dfm init dotfiles

# a sane pointer ("." means "this directory") still works
echo "." >dotfiles/.dfm_root
dfm init dotfiles
assert -f "$PWD/.config/dfm/config.toml"

