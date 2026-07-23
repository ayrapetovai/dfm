# B3a1δ — provoke read_link failure by replacing target symlink with a regular file

dfm init dotfiles

mkdir -p real_files
echo "target" > "real_files/other.txt"
ln -s "real_files/other.txt" "mylink"

dfm add mylink
assert_source "mylink.symlink"
assert -L mylink

# replace the target symlink with a regular file
rm mylink
echo "now a regular file" > mylink

# now forget by providing the source symlink file path
# this goes through B3a1: canonicalize succeeds, path in source dir,
# ends with symlink_postfix, target_symlink_abs_path exists,
# but read_link fails because it's not a symlink
assert_fail dfm forget "$PWD/dotfiles/mylink.symlink"

# source symlink file should still exist (forget failed)
assert_source "mylink.symlink"
# target regular file should still exist
assert -f mylink
assert_content_eq "mylink" "now a regular file"
