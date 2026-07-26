# Wildcard component matching: pattern ".*abc\.c" must match any
# component ending with "abc.c", both "abc.c" and "the-abc.c"

dfm init dotfiles
dfm ignore --patterns '.*abc\.c'

write "content" abc.c
write "other" the-abc.c

# Both files have a component ending with "abc.c" → both blocked
dfm add abc.c
assert_no_source "abc.c"

dfm add the-abc.c
assert_no_source "the-abc.c"
