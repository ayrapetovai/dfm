# Regression: `dfm add` on an explicitly-named symlink located *outside* the
# target directory must not touch the source dir or state. Originally it wrote
# a `.symlink` pointer outside the source directory and injected a `..`-escaping
# key into state.toml; now the scope check rejects the path outright.

dfm init dotfiles

# the unmanaged dfm config file is out of scope here ("No files managed" below)
dfm ignore .config/dfm

# Create a target outside the target directory ($PWD = target dir); use a
# unique name since $PWD/.. is the shared /tmp parent of the temp HOME.
OUTSIDE_DIR="$PWD/../dfm_outside_$(uuid)"
mkdir -p "$OUTSIDE_DIR"
echo "pointee" >"$OUTSIDE_DIR/target.txt"
ln -s "$OUTSIDE_DIR/target.txt" "$OUTSIDE_DIR/link"

# add by path must reject the outside symlink
run_fail dfm add "$OUTSIDE_DIR/link"
assert_succ grep -qF "outside the target directory" <<<"$FAIL_OUTPUT"

# postconditions: no pointer file anywhere under the source dir
assert_no_source "link.symlink"
[ ! -e "$OUTSIDE_DIR/link.symlink" ]

# state file must not contain the escaping key and all later commands work
RES=$(dfm status 2>&1)
assert_fail grep -q "escapes the source directory" <<<"$RES"
dfm status 2>/dev/null | assert_succ grep -q "No files managed"

# sanity: a symlink *inside* the target dir is still managed normally
ln -s "$OUTSIDE_DIR/target.txt" "$PWD/in_link"
