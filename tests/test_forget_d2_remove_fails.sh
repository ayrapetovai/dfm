# D2 — provoke fs::remove_file error by making the source directory read-only

CLEANUP() {
  chmod -R u+w "$PWD/dotfiles" 2>/dev/null || true
}
trap CLEANUP EXIT

dfm init dotfiles
write "content" file.txt
dfm add file.txt

# make the source directory read-only — prevents file deletion
chmod a-w "$PWD/dotfiles"

# forget must fail because fs::remove_file can't delete the source file
assert_fail dfm forget file.txt

# restore permissions so cleanup works
CLEANUP

# target file must still exist
assert -f file.txt
assert_content_eq "file.txt" "content"
