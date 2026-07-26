# `dfm merge` with encrypted sources: decrypts before merging,
# then writes the merged result to both target and source.
# (The --merge flag has been removed from `pull`; use the standalone
# `merge` subcommand instead.)

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"
MERGED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# configure a merge tool that copies the expected merged content to the result file
write "$MERGED" "$PWD/expected_merged"
dfm config --set merge_tool_command "cp $PWD/expected_merged {result}"

# Scenario 1: BothModified — dfm merge decrypts encrypted source
# and merges into the target
write "$ORIGINAL" secret.txt
dfm add --encrypt secret.txt
assert_encrypted "secret.txt" "$ORIGINAL"

rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$ORIGINAL"

# modify target
write "$MODIFIED" secret.txt

# modify encrypted source by adding a new version of secret.txt encrypted
write "$MODIFIED" new_version.txt
dfm add --encrypt --force new_version.txt
mv "$PWD/dotfiles/new_version.txt.encrypted" "$PWD/dotfiles/secret.txt.encrypted"
rm -f new_version.txt

# BothModified — dfm merge should decrypt source, run merge tool, copy result to both
dfm merge secret.txt

# merge tool wrote $MERGED to {result}, result goes to both target and source
assert_content_eq "secret.txt" "$MERGED"

# Scenario 2: TargetModified — pull fails without --force, overwrites with --force
write "$ORIGINAL" secret.txt
dfm add --encrypt --force secret.txt
assert_encrypted "secret.txt" "$ORIGINAL"

rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$ORIGINAL"

# modify only the target (source is unchanged)
write "$MODIFIED" secret.txt

# pull without --force must fail
assert_fail dfm pull secret.txt 2>/dev/null

# pull with --force must overwrite target with decrypted source
dfm pull --force secret.txt
assert_content_eq "secret.txt" "$ORIGINAL"

# Scenario 3: merge tool fails — error
write "$ORIGINAL" secret.txt
dfm add --encrypt --force secret.txt
rm secret.txt
dfm pull

# modify both to create BothModified
write "$MODIFIED" secret.txt
touch "$PWD/dotfiles/secret.txt.encrypted"

dfm config --set merge_tool_command "false"
assert_fail dfm merge secret.txt 2>/dev/null
