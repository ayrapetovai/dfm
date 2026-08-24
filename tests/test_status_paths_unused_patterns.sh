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

# ------------------------------------------------------------------
# Filter-flag reports show ONLY their own lists: the unused-patterns
# block appears solely in the unfiltered report (and via --unused-patterns).
# ------------------------------------------------------------------

# !? fixture: a managed file whose target copy was removed
write "gone" "gone.txt"
dfm add gone.txt
rm gone.txt

RES=$(dfm status --modified 2>/dev/null)
assert_succ grep -qF "Changes to add" <<<"$RES"
assert_fail grep -qF "Unused ignore patterns" <<<"$RES"

RES=$(dfm status --unpulled 2>/dev/null)
assert_succ grep -qF "Unpulled" <<<"$RES"
assert_fail grep -qF "Unused ignore patterns" <<<"$RES"

RES=$(dfm status --managed 2>/dev/null)
assert_succ grep -qF ".config/a/f.txt" <<<"$RES"
assert_fail grep -qF "Unused ignore patterns" <<<"$RES"

# porcelain: a filtered run with an empty list must not fall back to !P lines
RES=$(dfm status --conflicted --porcelain 2>/dev/null)
assert_fail grep -qF '!P' <<<"$RES"
assert "$RES" = ""

# the unfiltered default report still carries the block
RES=$(dfm status 2>/dev/null)
assert_succ grep -qF "Unused ignore patterns" <<<"$RES"

