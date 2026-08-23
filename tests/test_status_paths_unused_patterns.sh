# Status: unused-pattern analysis is a full-tree operation. A scoped report
# must not judge a pattern against only the requested paths: a pattern used
# *outside* the scope would otherwise look unused and trigger a false
# "Unused ignore patterns" block. Scoped reports never emit the block; only
# the explicitly global `--unused-patterns` flag performs the full walk.

dfm init dotfiles

mkdir -p .config/a
write "x" ".config/a/f.txt"
dfm add .config/a/f.txt
write "modified" ".config/a/f.txt"

# The in-use pattern prunes a directory from the walk; the genuinely unused
# one matches nothing anywhere in the target.
mkdir -p .cache
write ".cache/somefile" "cache-data"
dfm ignore -p "\.cache"
dfm ignore -p "/never/matches.*"

# Full report flags the genuinely unused one, but not the in-use `.cache`.
dfm status 2>/dev/null | grep -qF "Unused ignore patterns"
dfm status 2>/dev/null | grep -qF '!P  /never/matches.*'

# Scoped report must NOT emit the block, even though `\.cache` matches nothing
# inside the scope.
RES=$(dfm status .config 2>/dev/null)
assert_fail grep -qF "Unused ignore patterns" <<<"$RES"

# Scoped + --unused-patterns still gives the global answer, in report block
# shape (header + indented entries): only the truly unused pattern is listed,
# and the in-use `.cache` one never appears.
RES=$(dfm status --unused-patterns .config 2>/dev/null)
assert_succ grep -qF "Unused ignore patterns" <<<"$RES"
assert_succ grep -qF '!P  /never/matches.*' <<<"$RES"
assert_fail grep -qF '\.cache' <<<"$RES"

# The flag without a path scope prints the same block
dfm status --unused-patterns 2>/dev/null | grep -qF "Unused ignore patterns"

