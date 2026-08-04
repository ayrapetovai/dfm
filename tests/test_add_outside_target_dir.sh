CONTENT="$(uuid)"

dfm init dotfiles

# create a file outside the target directory ($PWD = target dir); use a unique
# directory name since $PWD/.. is the shared /tmp parent of the temp HOME
OUTSIDE_DIR="$PWD/../dfm_outside_$(uuid)"
mkdir -p "$OUTSIDE_DIR"
write "$CONTENT" "$OUTSIDE_DIR/file.txt"

# add by path should skip files outside target directory (exit 0, no source)
assert_succ dfm add "$OUTSIDE_DIR/file.txt"

# postcondition: no source file was created
assert_no_source "other/file.txt"

# cleanup
rm -rf "$OUTSIDE_DIR"
