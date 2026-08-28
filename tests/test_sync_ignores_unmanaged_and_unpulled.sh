# sync leaves files that exist on one side only untouched:
# unmanaged (target-only), never-synced (both exist, no record), unpulled (source-only).

ORIGINAL="$(uuid)"

dfm init dotfiles
write "$ORIGINAL" file.txt
dfm add file.txt

# unmanaged: a target file with no source counterpart at all
write "unmanaged" unmanaged.txt

# unpulled: a source file whose target does not exist
write "unpulled" "unpulled.txt"
dfm add unpulled.txt
rm unpulled.txt

# never-synced: both exist but no sync record — write directly to source
write "never-synced" "$PWD/dotfiles/manual.txt"
write "manual-target" manual.txt

# sync must succeed and touch none of these
assert_succ dfm sync

# unchanged
assert_content_eq "unmanaged.txt" "unmanaged"
assert_content_eq "manual.txt" "manual-target"
assert_content_eq "$PWD/dotfiles/manual.txt" "never-synced"
assert_fail test -e "unpulled.txt"
