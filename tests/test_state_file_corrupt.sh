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
run_fail dfm status
assert_succ grep -qF "state file is corrupt" <<<"$FAIL_OUTPUT"

# `add` must also fail (hard error, exit non-zero) rather than silently
# overwriting the corrupt state
run_fail dfm add file.txt
assert_succ grep -qF "state file is corrupt" <<<"$FAIL_OUTPUT"
