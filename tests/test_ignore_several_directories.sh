dfm init dotfiles
mkdir -p dir1/a
mkdir -p dir2/b
mkdir -p dir3/b
mkdir not_ignored_dir
write "text" file.txt

dfm ignore dir1
dfm ignore dir2 dir3

# Each ignored directory renders exactly once as a collapsed `!! dir/` entry;
# the plain file stays unmanaged. (The old check `! grep -q 'dir1|dir2|dir3'`
# was doubly broken: BRE treats `|` literally, and `!` exempts from errexit.)
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "??  file.txt" <<<"$RES"
assert_succ grep -qF "!!  dir1/" <<<"$RES"
assert_succ grep -qF "!!  dir2/" <<<"$RES"
assert_succ grep -qF "!!  dir3/" <<<"$RES"
assert_fail grep -qE '^\?\?  dir' <<<"$RES"
