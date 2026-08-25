CONTENT="$(uuid)"

dfm init dotfiles

# create a file outside the target directory ($PWD = target dir); use a unique
# directory name since $PWD/.. is the shared /tmp parent of the temp HOME
OUTSIDE_DIR="$PWD/../dfm_outside_$(uuid)"
mkdir -p "$OUTSIDE_DIR"
write "$CONTENT" "$OUTSIDE_DIR/file.txt"

# add by path must reject files outside target/source directories
run_fail dfm add "$OUTSIDE_DIR/file.txt"
assert_succ grep -qF "outside the target directory" <<<"$FAIL_OUTPUT"

# postcondition: no source file was created
assert_no_source "other/file.txt"

# cleanup
rm -rf "$OUTSIDE_DIR"
