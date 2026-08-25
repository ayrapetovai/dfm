# Trailing-slash ignore: "dfm ignore dirname/" must behave like
# "dfm ignore dirname" — ignore the directory and everything in it.

dfm init dotfiles
mkdir -p dirname/sub
write "a" dirname/a.txt
write "b" dirname/sub/b.txt
write "top" top.txt

dfm ignore dirname/

# the directory is stored as an escaped target-relative record
# (the trailing slash is insignificant and not kept)
assert_succ grep -q '^dirname$' "$XDG_STATE_HOME/dfm/ignore_file"

# files under dirname are ignored (not added)
dfm add dirname/a.txt
assert_no_source "dirname/a.txt"
dfm add dirname/sub/b.txt
assert_no_source "dirname/sub/b.txt"

# unrelated file is not affected
dfm add top.txt
assert_source "top.txt"
