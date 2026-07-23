# B1a / B1b1 — forget symlink-managed files

# --- scenario 1: an already-existing symlink, pointee inside source dir ---
dfm init dotfiles

CONTENT="$(uuid)"
write "$CONTENT" file.txt

# manually create a symlink pointing into the source dir
# (mimics what dfm add --symlink would do if it were implemented)
dfm add file.txt
rm file.txt
ln -s "$PWD/dotfiles/file.txt" "file.txt"
assert -L file.txt

dfm forget file.txt

# target symlink must be removed (B1a: pointee in source dir)
assert_fail test -f file.txt
assert_fail test -L file.txt

# --- scenario 2: symlink pointing outside source dir, managed via .symlink file ---
mkdir -p real_files
echo "real content" > "real_files/other.txt"
ln -s "real_files/other.txt" "mylink"

dfm add mylink
assert_source "mylink.symlink"
assert_content_eq "$PWD/dotfiles/mylink.symlink" "real_files/other.txt"
assert -L mylink

dfm forget mylink

# source symlink file must be removed (B1b1: content matches pointee)
assert_no_source "mylink.symlink"
# target symlink stays (pointee outside source dir, B1a didn't fire)
assert -L mylink
