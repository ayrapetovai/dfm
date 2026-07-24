# Init --dry-run must not create the source directory or any files.

# dry-run with non-existent source dir — nothing created
dfm init --dry-run dotfiles
assert_fail test -d dotfiles
assert_fail test -f "$PWD/.config/dfm/config.toml"

# also test the global --dry-run flag
dfm -n init dotfiles
assert_fail test -d dotfiles

# verify actual init still works after dry runs
dfm init dotfiles
assert -d dotfiles
assert -f "$PWD/.config/dfm/config.toml"
