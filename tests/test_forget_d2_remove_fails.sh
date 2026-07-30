# D2 — provoke fs::remove_file error by making the source directory read-only
# The command must still succeed (state entry removed) even when deletion fails

CLEANUP() {
  chmod -R u+w "$PWD/dotfiles" 2>/dev/null || true
}
trap CLEANUP EXIT

dfm init dotfiles
write "content" file.txt
dfm add file.txt

# make the source directory read-only — prevents file deletion
chmod a-w "$PWD/dotfiles"

# forget must succeed: state entry is removed even if deletion fails
dfm forget file.txt

# restore permissions so cleanup works
CLEANUP

# target file must still exist
assert -f file.txt
assert_content_eq "file.txt" "content"

# state entry must be removed — file should now be unmanaged
dfm status --porcelain | grep -q "^??"
