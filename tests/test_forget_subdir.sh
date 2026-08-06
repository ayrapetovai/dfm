# When forgetting subdirectory all other directories:
# parent and same level dirs - must remain.

mkdir -p d1/d2/d3
write "abc" "d1/d2/d3/file"
write "abc" "d1/d2/file"
write "abc" "d1/file"

# for no './' prefix
assert_succ dfm init dotfiles
assert_succ dfm add
assert_succ dfm forget "d1/d2/d3"
assert_no_source "d1/d2/d3/file"
assert_source "d1/d2/file"
assert_source "d1/file"

# for './' prefix
assert_succ dfm purge
assert_succ dfm init dotfiles
assert_succ dfm add
assert_succ dfm forget "./d1/d2/d3"
assert_no_source "d1/d2/d3/file"
assert_source "d1/d2/file"
assert_source "d1/file"

# second level subdirectory
assert_succ dfm purge
assert_succ dfm init dotfiles
assert_succ dfm add
assert_succ dfm forget "./d1/d2/"
assert_no_source "d1/d2/d3/file"
assert_no_source "d1/d2/file"
assert_source "d1/file"

# for dot-directory
mkdir -p .d1/d2/d3
write "abc" ".d1/d2/d3/file"
write "abc" ".d1/d2/file"
write "abc" ".d1/file"

assert_succ dfm purge
assert_succ dfm init dotfiles
assert_succ dfm add
assert_succ dfm forget "./.d1/d2/"
assert_no_source "dot_d1/d2/d3/file"
assert_no_source "dot_d1/d2/file"
assert_source "dot_d1/file"
