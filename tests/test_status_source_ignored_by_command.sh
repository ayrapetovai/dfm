# M1 — a source-side ignore added via `dfm ignore <source path>` stores the
# pattern in dotless target form (`^file\.txt$`), so status/diff must match the
# decoded form of the state key too (status shows `!!`, diff --all skips it).

dfm init dotfiles
echo "content" >file.txt
dfm add file.txt
assert_source "file.txt"

# source-side path -> record lands in the source ignore file as `^file\.txt$`
dfm ignore dotfiles/file.txt
assert_succ grep -qF '^file\.txt$' dotfiles/.dfm_ignore_file

# status must classify it as ignored even though both copies still exist
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qE '^  !!  file\.txt' <<<"$RES"

# diff --all must skip it entirely
RES=$(dfm diff --all 2>&1)
assert_fail grep -qF "file.txt" <<<"$RES"

