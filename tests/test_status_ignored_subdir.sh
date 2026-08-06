dfm init dotfiles

mkdir -p .config/dir1
mkdir -p .config/dir2
mkdir -p .config/dir3

write "abc" .config/dir1/file
write "abc" .config/dir2/file
write "abc" .config/dir3/file

dfm ignore .config/dir2
RES=$(dfm status)

assert_succ grep -q '.config/dir1/file' <<<"$RES"
assert_succ grep -q '.config/dir3/file' <<<"$RES"
assert_fail grep -q 'config/\*' <<<"$RES"
