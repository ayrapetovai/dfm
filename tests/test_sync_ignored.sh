# sync respects the target ignore file: an ignored managed file is left
# untouched. With --force the ignore is overridden and the file is synced.

dfm init dotfiles
write "v1" ignored.txt
dfm add ignored.txt            # managed + synced
dfm ignore ignored.txt         # now also ignored

write "v2" ignored.txt         # target change while ignored

# plain sync must skip the ignored file
dfm sync
assert_content_eq "$PWD/dotfiles/ignored.txt" "v1"

# with --force the ignore is overridden and the target change is pushed
dfm sync --force
assert_content_eq "$PWD/dotfiles/ignored.txt" "v2"
