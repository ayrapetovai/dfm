# dfm pull must decrypt files that were previously added with --encrypt.
#
# 1. Create a directory with files (including nested subdirectory)
# 2. `dfm add --encrypt` to encrypt them into the source dir
# 3. Remove the target files
# 4. `dfm pull` — should traverse source, find .encrypted files, decrypt them
# 5. Verify decrypted content matches originals

PASSWORD="$(uuid)"
CONTENT_A="$(uuid)"
CONTENT_B="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# create a directory with files (including nested subdirectory)
mkdir -p dir/sub
write "$CONTENT_A" dir/a.txt
write "$CONTENT_B" dir/sub/b.txt

# encrypt the whole directory
dfm add -e dir

# remove target files to simulate a fresh pull
rm dir/a.txt
rm dir/sub/b.txt
rmdir dir/sub dir 2>/dev/null || true

# pull should decrypt files back
dfm pull

# verify decrypted content is correct
assert_content_eq "dir/a.txt" "$CONTENT_A"
assert_content_eq "dir/sub/b.txt" "$CONTENT_B"

# --- regression: non-encrypted files still pull normally ---
mkdir -p other_dir
write "plain-content" other_dir/plain.txt
dfm add other_dir

# remove and pull back
rm other_dir/plain.txt
dfm pull
assert_content_eq "other_dir/plain.txt" "plain-content"
