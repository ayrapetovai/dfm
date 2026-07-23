# dfm add --symlink on a regular file must:
#   1. Copy the file to source (like normal add)
#   2. Replace the original target file with a symlink pointing to the new source file

CONTENT="$(uuid)"

dfm init dotfiles

# --- scenario 1: basic add --symlink ---
write "$CONTENT" regular.txt
dfm add --symlink regular.txt

# source got the file content
assert_source "regular.txt"

# target is now a symlink
assert -L regular.txt

# symlink points to the source file
assert "$PWD/dotfiles/regular.txt" = "$(readlink -f regular.txt)"

# content is preserved
assert_content_eq "regular.txt" "$CONTENT"

# --- scenario 2: idempotent re-run (symlink already valid) ---
dfm add --symlink regular.txt

# still a symlink pointing to the right place
assert -L regular.txt
assert "$PWD/dotfiles/regular.txt" = "$(readlink -f regular.txt)"

# --- scenario 3: --dry-run leaves the target as-is ---
OTHER_CONTENT="$(uuid)"
write "$OTHER_CONTENT" other.txt
dfm add --symlink --dry-run other.txt

# source file should NOT exist
assert_no_source "other.txt"

# target is still a regular file, not a symlink
assert_fail test -L other.txt
assert -f other.txt
assert_content_eq "other.txt" "$OTHER_CONTENT"

# --- scenario 4: add --symlink with --encrypt is rejected ---
write "$CONTENT" noenc.txt
assert_fail dfm add --symlink --encrypt noenc.txt

# nothing was added
assert_no_source "noenc.txt"
assert_fail test -L noenc.txt
assert -f noenc.txt
