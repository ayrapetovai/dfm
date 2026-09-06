# Status filter flags combine additively: each enabled flag contributes its own
# set and the report shows the union — `--managed --unmanaged` shows BOTH
# blocks, with no "contradictory flags" error and no priority between flags.
# Short/long spellings in any order are interchangeable. The only filter that
# overrides the others is `--encrypted`.

dfm init dotfiles

# --- setup: one file per state ---
# BothModified
write "a" "both.txt"
dfm add both.txt
write "b" "both.txt"
write "c" "$PWD/dotfiles/both.txt"

# TargetModified
write "d" "target_only.txt"
dfm add target_only.txt
write "e" "target_only.txt"

# SourceModified
write "f" "source_mod.txt"
dfm add source_mod.txt
write "g" "$PWD/dotfiles/source_mod.txt"

# Up to date
write "h" "uptodate.txt"
dfm add uptodate.txt

# Unmanaged
write "i" "unmanaged.txt"

# Unpulled (target removed, source kept)
write "j" "unpulled.txt"
dfm add unpulled.txt
rm unpulled.txt

# Ignored directory
mkdir ignored_dir
write "k" "ignored_dir/file.txt"
dfm ignore -p "ignored_dir/"

# Encrypted, synced
PW="$(uuid)"
write "secret" "secret.txt"
dfm config --set obtain_password_shell_command "echo -n $PW"
dfm add -e secret.txt

# --- union: --managed --unmanaged shows both blocks, exit 0 ---
RES=$(dfm status --managed --unmanaged 2>/dev/null)
assert_succ grep -qF "both.txt" <<<"$RES"
assert_succ grep -qF "uptodate.txt" <<<"$RES"
assert_succ grep -qF "unmanaged.txt" <<<"$RES"
# ignored is in neither requested set
assert_fail grep -qF "ignored_dir" <<<"$RES"
# Unpulled is managed, but its block stays behind --unpulled/--all in the
# grouped report (pre-existing rule, unchanged by additivity).
assert_fail grep -qF "unpulled.txt" <<<"$RES"

# --- union: --modified --unpulled shows modified AND unpulled ---
RES=$(dfm status --modified --unpulled 2>/dev/null)
assert_succ grep -qF "both.txt" <<<"$RES"
assert_succ grep -qF "target_only.txt" <<<"$RES"
assert_succ grep -qF "source_mod.txt" <<<"$RES"
assert_succ grep -qF "unpulled.txt" <<<"$RES"
assert_fail grep -qF "unmanaged.txt" <<<"$RES"
assert_fail grep -qF "uptodate.txt" <<<"$RES"

# --- union: --ignored --modified shows both blocks ---
RES=$(dfm status --ignored --modified 2>/dev/null)
assert_succ grep -qF "both.txt" <<<"$RES"
assert_succ grep -qF "ignored_dir/" <<<"$RES"
assert_fail grep -qF "unmanaged.txt" <<<"$RES"

# --- order and spelling are interchangeable (byte-identical porcelain) ---
A=$(dfm status --modified --unmanaged --porcelain 2>/dev/null)
B=$(dfm status -m -U --porcelain 2>/dev/null)
assert_succ [ "$A" = "$B" ]
C=$(dfm status --managed --unmanaged --porcelain 2>/dev/null)
D=$(dfm status --unmanaged --managed --porcelain 2>/dev/null)
assert_succ [ "$C" = "$D" ]

# --- all filters together run cleanly ---
dfm status --conflicted --modified --managed --unmanaged --unpulled --ignored \
  2>/dev/null | grep -qF "both.txt"

# --- -e overrides other filters even when combined additively ---
RES=$(dfm status --encrypted --unmanaged 2>/dev/null)
assert_succ grep -qF "secret.txt" <<<"$RES"
assert_fail grep -qF "unmanaged.txt" <<<"$RES"
assert_fail grep -qF "both.txt" <<<"$RES"

RES=$(dfm status -e --modified --unpulled 2>/dev/null)
assert_succ grep -qF "secret.txt" <<<"$RES"
assert_fail grep -qF "both.txt" <<<"$RES"
assert_fail grep -qF "unpulled.txt" <<<"$RES"