# H1 — ignore requires paths or a flag with parameters
dfm init dotfiles

# no arguments at all → clap rejects
assert_fail dfm ignore

# only a dry-run flag → still no input → clap rejects
assert_fail dfm ignore -n

# -p without any pattern values → clap rejects
assert_fail dfm ignore -p

# -r without any records → clap rejects
assert_fail dfm ignore -r

# valid invocation still works
dfm ignore -p '\.txt$'
