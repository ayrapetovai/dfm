# dfm ignore with a missing source directory must fail with a clear error
# instead of silently doing nothing.

dfm init dotfiles

# remove the source directory entirely
rm -rf "$PWD/dotfiles"

run_fail dfm ignore -p ".*"
assert_succ grep -qF "source directory does not exist" <<<"$FAIL_OUTPUT"
assert_succ grep -qF "dfm init" <<<"$FAIL_OUTPUT"
