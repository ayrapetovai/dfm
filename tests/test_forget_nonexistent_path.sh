# dfm forget of a path that exists nowhere — neither in the target directory
# nor in the source directory — must fail with "<path> does not exist"
# instead of silently succeeding.

dfm init dotfiles

# nonexistent target-side path
set +e
dfm forget ghost.txt 2>err.txt
rc=$?
set -e
assert_fail test $rc -eq 0
assert_succ grep -qF "ghost.txt does not exist" err.txt

# nonexistent source-side path whose target does not exist either
set +e
dfm forget dotfiles/dot_ghost.txt 2>err.txt
rc=$?
set -e
assert_fail test $rc -eq 0
assert_succ grep -qF "dotfiles/dot_ghost.txt does not exist" err.txt

# several paths, one missing: the run fails and nothing is forgotten
write "managed" managed.txt
dfm add managed.txt
set +e
dfm forget managed.txt another_ghost.txt 2>err.txt
rc=$?
set -e
assert_fail test $rc -eq 0
assert_succ grep -qF "another_ghost.txt does not exist" err.txt
assert_source managed.txt

# sanity: an existing managed file is still forgotten normally
dfm forget managed.txt
assert_no_source managed.txt

rm -f err.txt
