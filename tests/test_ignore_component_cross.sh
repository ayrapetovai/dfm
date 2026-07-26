# Cross-component matching: pattern ".*abc/def.*" matches adjacent
# "abc/define" but NOT "abc/x/define" where "x" separates them.

dfm init dotfiles
mkdir -p abc
mkdir -p abc/x
dfm ignore --patterns '.*abc/def.*'

write "adjacent" abc/define
write "separated" abc/x/define

# abc/define has adjacent components ["abc", "define"] → blocked
dfm add abc/define
assert_no_source "abc/define"

# abc/x/define has components ["abc", "x", "define"] → not adjacent → NOT blocked
dfm add abc/x/define
assert_source "abc/x/define"
