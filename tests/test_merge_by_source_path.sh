# dfm merge PATH with a source directory path infers the target path
# (source-path branch — same logic as pull.rs source-traversal).

CONTENT="$(uuid)"
MODIFIED="$(uuid)"
SOURCE_MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set merge_tool_command "cp {target} {result}"

# ------------------------------------------------------------------
# Setup: create clean state
# ------------------------------------------------------------------
write "$CONTENT" file.txt
dfm add file.txt
assert_source "file.txt"

# modify both
write "$MODIFIED" file.txt
write "$SOURCE_MODIFIED" "$PWD/dotfiles/file.txt"

# ------------------------------------------------------------------
# Act: source-path branch — path starts with source_dir_abs_path
# ------------------------------------------------------------------
dfm merge "$PWD/dotfiles/file.txt"

# ------------------------------------------------------------------
# Assert: merge tool kept target version
# ------------------------------------------------------------------
assert_content_eq "file.txt" "$MODIFIED"
assert_content_eq "$PWD/dotfiles/file.txt" "$MODIFIED"
