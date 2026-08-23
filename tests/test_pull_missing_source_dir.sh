CONTENT="$(uuid)"
# dfm pull with a missing source directory must fail with a clear error,
# not panic or produce a confusing NotFound error.

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# remove the source directory entirely
rm -rf "$PWD/dotfiles"

run_fail dfm pull
assert_succ grep -qF "source directory does not exist" <<<"$FAIL_OUTPUT"
assert_succ grep -qF "dfm init" <<<"$FAIL_OUTPUT"
