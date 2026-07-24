# dfm pull --merge with encrypted sources: decrypts before merging,
# then writes the merged result to the target.

PASSWORD="$(uuid)"
ORIGINAL="$(uuid)"
MODIFIED="$(uuid)"
MERGED="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# configure a merge tool that copies the expected merged content to the result file
write "$MERGED" "$PWD/expected_merged"
dfm config --set merge_tool_command "cp $PWD/expected_merged {result}"

# ---------------------------------------------------------------
# Scenario 1: BothModified — pull --merge decrypts encrypted source
# and merges into the target
# ---------------------------------------------------------------
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

# BothModified — pull --merge should decrypt source, run merge tool, copy result to target
dfm pull --merge secret.txt

# merge tool wrote $MERGED to {result}, result goes to both target and source
assert_content_eq "secret.txt" "$MERGED"

# ---------------------------------------------------------------
# Scenario 2: TargetModified — pull --merge resolves via merge tool
# ---------------------------------------------------------------
write "$ORIGINAL" secret.txt
dfm add --encrypt --force secret.txt
assert_encrypted "secret.txt" "$ORIGINAL"

rm secret.txt
dfm pull
assert_content_eq "secret.txt" "$ORIGINAL"

# modify only the target (source is unchanged)
write "$MODIFIED" secret.txt

dfm pull --merge secret.txt

# merge tool wrote $MERGED to {result}, which is copied to both target and source
assert_content_eq "secret.txt" "$MERGED"

# ---------------------------------------------------------------
# Scenario 3: merge tool fails — error
# ---------------------------------------------------------------
write "$ORIGINAL" secret.txt
dfm add --encrypt --force secret.txt
rm secret.txt
dfm pull
write "$MODIFIED" secret.txt

dfm config --set merge_tool_command "false"
assert_fail dfm pull --merge secret.txt 2>/dev/null
