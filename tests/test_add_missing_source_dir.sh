CONTENT="$(uuid)"
# dfm add with a missing source directory must fail with a clear error and
# must not auto-create an incomplete source directory.

dfm init dotfiles

# remove the source directory entirely
rm -rf "$PWD/dotfiles"

write "$CONTENT" file.txt

set +e
dfm add file.txt 2>err.txt
rc=$?
set -e

assert_fail test $rc -eq 0
grep -q "source directory does not exist" err.txt
grep -q "dfm init" err.txt

# the source directory must not be recreated
assert_fail test -d "$PWD/dotfiles"
rm -f err.txt
