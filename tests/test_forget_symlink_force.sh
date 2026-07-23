# B1b2 / B1b3 — symlink source file content mismatches, with/without --force
dfm init dotfiles

# create a symlink that points outside source dir
mkdir -p real_files
echo "target" > "real_files/other.txt"
ln -s "real_files/other.txt" "mylink"

dfm add mylink
assert "real_files/other.txt" = "$(cat "$PWD/dotfiles/mylink.symlink")"
assert -L mylink

# modify the source symlink file to point somewhere else
echo "different/pointee" > "$PWD/dotfiles/mylink.symlink"

# forget without --force → source symlink file should NOT be removed (B1b3)
dfm forget mylink
assert -f "$PWD/dotfiles/mylink.symlink"
# target symlink stays (it doesn't point into source dir)
assert -L mylink

# forget with --force → source symlink file should be removed (B1b2)
dfm forget --force mylink
assert_fail test -f "$PWD/dotfiles/mylink.symlink"
assert -L mylink
