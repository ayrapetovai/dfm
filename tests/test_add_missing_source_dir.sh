CONTENT="$(uuid)"
# dfm add with a missing source directory must fail with a clear error and
# must not auto-create an incomplete source directory.

dfm init dotfiles

# remove the source directory entirely
rm -rf "$PWD/dotfiles"

write "$CONTENT" file.txt

run_fail dfm add file.txt
assert_succ grep -qF "source directory does not exist" <<<"$FAIL_OUTPUT"
assert_succ grep -qF "dfm init" <<<"$FAIL_OUTPUT"

# the source directory must not be recreated
assert_fail test -d "$PWD/dotfiles"
