# A `dfm pull` must skip an unreadable target file with a warning while still
# updating the readable files; the command must not fail, and the pending
# update is applied once the target becomes readable again.

dfm init dotfiles

write "aaa-v1" aaa.txt
write "bbb-v1" bbb.txt
dfm add aaa.txt bbb.txt
dfm pull

# modify both sources, then make bbb.txt unreadable
write "aaa-v2" dotfiles/aaa.txt
write "bbb-v2" dotfiles/bbb.txt
chmod 000 bbb.txt

dfm pull 2>&1 | grep -q "skipping unreadable path"

# the readable target was updated; restore permissions to inspect the
# unreadable one, which kept its old content
assert_content_eq "aaa.txt" "aaa-v2"
chmod 644 bbb.txt
assert_content_eq "bbb.txt" "bbb-v1"

# a subsequent pull applies the pending update
dfm pull
assert_content_eq "bbb.txt" "bbb-v2"
