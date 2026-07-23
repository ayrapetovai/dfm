# dfm add --encrypt on a directory must encrypt each file inside it,
# preserving the relative directory structure in the source.

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

# source got encrypted files with correct relative paths
assert_source "dir/a.txt.encrypted"
assert_source "dir/sub/b.txt.encrypted"

# no plain copies exist in source
assert_no_source "dir/a.txt"
assert_no_source "dir/sub/b.txt"

# decrypt and verify content of the flat file
rm dir/a.txt
7z -p"$PASSWORD" x -y "$PWD/dotfiles/dir/a.txt.encrypted" > /dev/null 2>&1
assert_content_eq "dir/a.txt" "$CONTENT_A"

# decrypt and verify content of the nested file
rm dir/sub/b.txt
7z -p"$PASSWORD" x -y "$PWD/dotfiles/dir/sub/b.txt.encrypted" > /dev/null 2>&1
assert_content_eq "dir/sub/b.txt" "$CONTENT_B"

# --- regression: non-encrypted add still works on a separate directory ---
mkdir -p other_dir
write "$CONTENT_A" other_dir/plain.txt
dfm add other_dir

# source got a plain copy
assert_source "other_dir/plain.txt"
assert_no_source "other_dir/plain.txt.encrypted"
assert_content_eq "$PWD/dotfiles/other_dir/plain.txt" "$CONTENT_A"
