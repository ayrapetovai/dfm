# forget with `.` path walks the current directory and removes all managed
# files from management, cleaning up both source files and state entries.
#
# State file assertions verify that every managed entry is removed from
# state.toml, so a subsequent `dfm pull` does not recreate anything.

dfm init dotfiles
STATE_FILE="$HOME/.local/state/dfm/state.toml"

write "root-a" root-a.txt
write "root-b" root-b.txt
write "nested" sub/dir/nested.txt

dfm add root-a.txt
dfm add root-b.txt
dfm add sub/dir/nested.txt

# confirm state entries exist
assert_succ grep -Fq '"root-a.txt" = ' "$STATE_FILE"
assert_succ grep -Fq '"root-b.txt" = ' "$STATE_FILE"
assert_succ grep -Fq '"sub/dir/nested.txt" = ' "$STATE_FILE"

dfm forget .

# source files must be removed
assert_no_source "root-a.txt"
assert_no_source "root-b.txt"
assert_no_source "sub/dir/nested.txt"

# target files must still exist
assert -f root-a.txt
assert -f root-b.txt
assert -f sub/dir/nested.txt
assert_content_eq "root-a.txt" "root-a"
assert_content_eq "root-b.txt" "root-b"
assert_content_eq "sub/dir/nested.txt" "nested"

# state entries must be removed
assert_fail grep -Fq '"root-a.txt" = ' "$STATE_FILE"
assert_fail grep -Fq '"root-b.txt" = ' "$STATE_FILE"
assert_fail grep -Fq '"sub/dir/nested.txt" = ' "$STATE_FILE"

# pull must not recreate anything
rm root-a.txt root-b.txt sub/dir/nested.txt
rmdir sub/dir sub
dfm pull
assert_fail test -f root-a.txt
assert_fail test -f root-b.txt
assert_fail test -f sub/dir/nested.txt
