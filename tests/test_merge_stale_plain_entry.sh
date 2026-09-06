# dfm merge PATH must not be shadowed by a stale state entry.
#
# Regression for the user-observed case: the state file can carry a leftover
# plain entry for a file that was later re-added encrypted (the plain source
# file no longer exists). `merge` used to match that exact plain key first and
# silently skipped the candidate (source file missing), never trying the live
# `.encrypted` entry — no merge tool ran and no scratch dir was created.
#
# The scenario is built exactly like the real user state: a live
# `secret.txt.encrypted` entry plus a stale `secret.txt` entry whose source
# file does not exist.

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"
SOURCE_MODIFIED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
dfm config --set merge_tool_command "cp {target} {result}"

# Live encrypted entry
write "$ORIGINAL" secret.txt
dfm add -e secret.txt
assert_encrypted "secret.txt" "$ORIGINAL"

# Simulate the stale plain entry a pre-encryption add left in the state file
# (its source file dotfiles/secret.txt does not exist) — same shape as the
# real `/experements/dfm-exp` state file.
cat >> .local/state/dfm/state.toml <<EOF
[syncs."secret.txt"]
mtime = "1700000000;1"
sha256 = "deadbeef"
EOF

# Conflict: both sides modified
rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$ORIGINAL"

write "$MODIFIED" secret.txt

write "$SOURCE_MODIFIED" v2.txt
dfm add -e --force v2.txt
mv "$PWD/dotfiles/v2.txt.encrypted" "$PWD/dotfiles/secret.txt.encrypted"
rm -f v2.txt

# Act: merge must resolve to the live .encrypted entry and run the tool
dfm merge secret.txt

# Assert: tool ran (target kept its version) and the encrypted source was
# re-encrypted with the merged content
assert_content_eq "secret.txt" "$MODIFIED"

rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$MODIFIED"