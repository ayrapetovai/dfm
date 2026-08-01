CONTENT="$(uuid)"
# dfm pull with a missing source directory must fail with a clear error,
# not panic or produce a confusing NotFound error.

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# remove the source directory entirely
rm -rf "$PWD/dotfiles"

set +e
dfm pull 2>err.txt
rc=$?
set -e

assert_fail test $rc -eq 0
grep -q "source directory does not exist" err.txt
grep -q "dfm init" err.txt

rm -f err.txt
