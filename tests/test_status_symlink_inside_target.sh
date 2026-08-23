CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt
assert_source file.txt

ln -s file.txt link-to-file.txt

RES=$(dfm status)
assert_succ grep -qF "link-to-file.txt" <<<"$RES"

dfm add link-to-file.txt
assert_source link-to-file.txt.symlink

# managed again → no longer reported as unmanaged
# (the old `grep -qv` passed whenever ANY line lacked the name)
RES=$(dfm status)
assert_fail grep -qF "link-to-file.txt" <<<"$RES"

rm link-to-file.txt file.txt

dfm pull
assert_content_eq file.txt "$CONTENT"
assert_content_eq link-to-file.txt "$CONTENT"
assert -L link-to-file.txt
