CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt
assert_source file.txt

ln -s file.txt link-to-file.txt

dfm status | grep -q link-to-file.txt
assert $? = 0

dfm add link-to-file.txt
assert_source link-to-file.txt.symlink

dfm status | grep -qv link-to-file.txt
assert $? = 0

rm link-to-file.txt file.txt

dfm pull
assert_content_eq file.txt "$CONTENT"
assert_content_eq link-to-file.txt "$CONTENT"
assert -L link-to-file.txt
