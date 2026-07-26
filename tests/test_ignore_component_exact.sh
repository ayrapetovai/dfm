# Exact component matching: pattern "abc\.c" must match a component
# exactly "abc.c" but not "the-abc.c"

dfm init dotfiles
dfm ignore --patterns 'abc\.c'

write "content" abc.c
write "other" the-abc.c

# abc.c is exactly "abc.c" → blocked by ignore
dfm add abc.c
assert_no_source "abc.c"

# the-abc.c is NOT exactly "abc.c" → NOT blocked
dfm add the-abc.c
assert_source "the-abc.c"
