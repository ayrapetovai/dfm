# Traversal must not step into ignored directories.
# - `dfm add` over the target dir prunes fully-ignored directories, so the
#   traversal-progress heartbeat (fires at 500 visited entries) never counts
#   the files inside them.
# - An explicitly named ignored dir is still entered (root is always walked),
#   so `add --force <ignored_dir>` keeps working.
# - `dfm status` renders a pruned dir as a single `!! dir/` entry.

dfm init dotfiles

# A large ignored directory — if the walk stepped into it, 600 files would be
# visited and the 500-entry progress heartbeat would fire.
mkdir -p big
for i in $(seq 1 600); do
    write "x" "big/f_$i.txt"
done
dfm ignore big

write "keep" keep.txt

# Capture stderr at -v 0: traversal progress renders here when ≥500 entries
# are visited. With pruning, far fewer entries are visited.
dfm -v 0 add 1>stdout.txt 2>stderr.txt
! grep -qF "traversing... 500 entries visited" stderr.txt

# ignored files were not added; the non-ignored file was
assert_no_source "big/f_1.txt"
assert_no_source "big/f_300.txt"
assert_source "keep.txt"

# status shows the pruned dir as a single !! entry, not its contents
dfm status --all --short 2>/dev/null | grep -qF "!! big/"
! dfm status --all --short 2>/dev/null | grep -qF "big/f_1.txt"

# an explicitly named ignored dir is still entered (root kept): --force adds it
mkdir -p explicit
write "e" "explicit/one.txt"
dfm ignore explicit
dfm add --force explicit 2>/dev/null
assert_source "explicit/one.txt"
