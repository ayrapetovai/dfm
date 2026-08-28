# sync restricted to explicit paths; relative paths anchor at the target dir;
# an out-of-scope path (outside the managed tree) is rejected.

dfm init dotfiles
mkdir -p sub
write "v1" sub/a.txt
write "v1" top.txt
dfm add sub/a.txt top.txt

# modify only top.txt
write "v2" top.txt
# also modify sub/a.txt so a full sync would touch it too
write "v2sub" sub/a.txt

# sync only top.txt: sub/a.txt must remain at its source value
dfm sync top.txt
dfm sync "$PWD/dotfiles/top.txt"
assert_content_eq "$PWD/dotfiles/top.txt" "v2"
assert_content_eq "$PWD/dotfiles/sub/a.txt" "v1"

# path that resolves outside the managed tree is rejected
run_fail dfm sync /etc
assert_succ grep -qF "outside the target directory" <<<"$FAIL_OUTPUT"
