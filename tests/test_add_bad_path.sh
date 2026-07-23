dfm init dotfiles

# add a non-existent path → must fail
assert_fail dfm add /nonexistent/path

# add with --force on a bad path → should still fail (invalid path, not a conflict)
assert_fail dfm add -f /nonexistent/path
