# dfm ignore with a missing source directory must fail with a clear error
# instead of silently doing nothing.

dfm init dotfiles

# remove the source directory entirely
rm -rf "$PWD/dotfiles"

set +e
dfm ignore -p ".*" 2>err.txt
rc=$?
set -e

assert_fail test $rc -eq 0
grep -q "source directory does not exist" err.txt
grep -q "dfm init" err.txt

rm -f err.txt
