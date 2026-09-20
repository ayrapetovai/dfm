# A state file whose source and target directories are the same (hand-edited
# or otherwise corrupted) must make every managed command refuse to run: with
# equal paths, "remove the source" == "remove the whole target directory", so
# purge/forget must never delete anything in that configuration.

CONTENT="$(uuid)"
write "$CONTENT" file.txt

# craft a state file with source_directory == target_directory == $HOME
STATE_FILE="$XDG_STATE_HOME/dfm/state.toml"
mkdir -p "$XDG_STATE_HOME/dfm"
write "target_directory = \"$PWD\"
source_directory = \"$PWD\"
[syncs]
" "$STATE_FILE"

# read-only managed commands refuse with a clear error
run_fail dfm status
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

run_fail dfm diff
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

# writing commands refuse before touching any file
run_fail dfm add file.txt
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

run_fail dfm pull
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

# destructive commands refuse without deleting anything
run_fail dfm forget --force file.txt
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

run_fail dfm purge
assert_succ grep -qF "same directory" <<<"$FAIL_OUTPUT"

# nothing was deleted
assert -f file.txt
assert -f "$STATE_FILE"

