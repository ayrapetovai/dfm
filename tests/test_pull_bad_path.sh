dfm init dotfiles

# pull a non-existent path → must fail
assert_fail dfm pull /nonexistent/path

# also with --force, a non-existent path should still fail (it's not a conflict)
assert_fail dfm pull -f /nonexistent/path
