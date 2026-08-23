# A state entry whose SOURCE path matches a .dfm_ignore_file pattern must not
# be offered as pullable: when the target copy is removed, status classifies
# it as ignored (!!), never as unpulled (!?). Regression for the bug where
# the source-side ignore file was consulted nowhere in status.
dfm init dotfiles

write "target version" README.md
dfm add README.md
assert_source "README.md"

# ignore the source copy after the fact (repo-internal doc, not home config)
echo '^README\.md$' >> dotfiles/.dfm_ignore_file

# already ignored while both copies exist
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "!!  README.md" <<<"$RES"
assert_fail grep -qF "!?" <<<"$RES"

# removing the target copy must NOT make it unpullable
rm "$PWD/README.md"
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qE '^  !!  README\.md' <<<"$RES"
assert_fail grep -qF "!?" <<<"$RES"

# and it stays out of the unpulled filter view entirely
RES=$(dfm status --unpulled 2>/dev/null)
assert_fail grep -qF "README.md" <<<"$RES"

# sanity: without a matching source pattern the same removal IS unpulled
write "other" other.txt
dfm add other.txt
rm "$PWD/other.txt"
dfm status --porcelain 2>/dev/null | grep -qF $'!?\tother.txt'
