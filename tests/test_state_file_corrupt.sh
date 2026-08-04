CONTENT="$(uuid)"
# A corrupt (unparseable) state file must be reported as such, not as a
# missing state file or an empty source path. The original parse error must
# survive so the user can fix their state file.

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# corrupt the state file
STATE_FILE="$HOME/.local/state/dfm/state.toml"
assert -f "$STATE_FILE"
echo "not valid toml {{{" >"$STATE_FILE"

# `status` must fail with a clear corrupt-state message
set +e
dfm status 2>err.txt
rc=$?
set -e

assert_fail test $rc -eq 0
assert_succ grep -q "state file is corrupt" err.txt

# `add` must also fail (hard error, exit non-zero) rather than silently
# overwriting the corrupt state
set +e
dfm add file.txt 2>err2.txt
rc=$?
set -e

assert_fail test $rc -eq 0
assert_succ grep -q "state file is corrupt" err2.txt

rm -f err.txt err2.txt
