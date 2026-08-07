# Status: a directory is only collapsed to {dir}/* when EVERY path under it
# belongs to the same status group. Any path of another status (an up-to-date
# tracked file, another group, or an ignored pruned dir) severs the fold, so
# files the user needs to see stay listed individually.

dfm init dotfiles

# --- Up-to-date (tracked) file severs the fold ---------------------------
# dir/ has two unmanaged-looking siblings, but one is already added (--),
# so the parent dir must NOT collapse; each file prints separately.
mkdir -p mixed
write "a" "mixed/one.txt"
write "b" "mixed/two.txt"
write "c" "mixed/three.txt"

dfm add mixed/two.txt

RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF '??  mixed/one.txt' <<<"$RES"
assert_succ grep -qF '??  mixed/three.txt' <<<"$RES"
assert_succ grep -qF -- '--  mixed/two.txt' <<<"$RES"
assert_fail grep -qF 'mixed/*' <<<"$RES"

dfm status 2>/dev/null | grep -qF '??  mixed/one.txt'
dfm status 2>/dev/null | grep -qF '??  mixed/three.txt'
RES=$(dfm status 2>/dev/null)
assert_fail grep -qF 'mixed/*' <<<"$RES"

rm -rf mixed

# --- An ignored sibling dir severs the fold (deep) -----------------------
# .config contains dir1 and dir3, but dir2 is ignored: the .config fold must
# not swallow the non-ignored sibling files.
mkdir -p .config/dir1 .config/dir2 .config/dir3
write "a" ".config/dir1/file"
write "b" ".config/dir2/file"
write "c" ".config/dir3/file"
dfm ignore .config/dir2 >/dev/null 2>&1

RES=$(dfm status 2>/dev/null)
assert_succ grep -qF ".config/dir1/file" <<<"$RES"
assert_succ grep -qF ".config/dir3/file" <<<"$RES"
assert_fail grep -qF 'config/*' <<<"$RES"

rm -rf .config

# --- No blockers: a genuinely shared dir still folds --------------------
# absense of any blocker must preserve the original fold behaviour so the
# snap/ + a.txt merge collapses to snap/*.
mkdir -p snap/dir1 plain
write "a" "snap/dir1/b.txt"
write "b" "snap/a.txt"
write "c" "plain/x.txt"
write "d" "plain/y.txt"

dfm status 2>/dev/null | grep -qF '??  snap/*'
dfm status 2>/dev/null | grep -qF '??  plain/*'
RES=$(dfm status 2>/dev/null)
assert_fail grep -qF 'snap/dir1/b.txt' <<<"$RES"
assert_fail grep -qF 'snap/a.txt' <<<"$RES"
assert_fail grep -qF 'plain/x.txt' <<<"$RES"
assert_fail grep -qF 'plain/y.txt' <<<"$RES"

rm -rf snap plain

# --- --all still folds a group even when another group exists ------------
# unmanaged a/b + a/c fold; it is independent of whether other files exist.
mkdir -p a/b a/c
write "1" "a/b/1.txt"
write "2" "a/b/2.txt"
write "3" "a/c/3.txt"

dfm status --all 2>/dev/null | grep -qF '??  a/*'
RES=$(dfm status --all 2>/dev/null)
assert_fail grep -qF 'a/b/1.txt' <<<"$RES"

rm -rf a

