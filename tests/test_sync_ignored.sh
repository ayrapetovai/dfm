# sync respects the target ignore file: an ignored managed file is left
# untouched, with AND without --force (sync never overrides the ignore).

dfm init dotfiles
write "v1" ignored.txt
dfm add ignored.txt            # managed + synced
dfm ignore ignored.txt         # now also ignored

write "v2" ignored.txt         # target change while ignored

# plain sync must skip the ignored file
dfm sync
assert_content_eq "$PWD/dotfiles/ignored.txt" "v1"

# --force must also skip the ignored file (sync never overrides the ignore)
dfm sync --force
assert_content_eq "$PWD/dotfiles/ignored.txt" "v1"

# the ignore pattern is untouched by either run
assert_succ grep -Fq 'ignored\.txt' "$XDG_STATE_HOME/dfm/ignore_file"
