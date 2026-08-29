# status default report marks encrypted entries with `(encrypted)`, right-aligned,
# including a wholly-encrypted folded dir (`dir/* (encrypted)`). --short/--porcelain
# stay clean (no marker).

PW="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PW"

# Wholly-encrypted directory -> folds to `.ssh/* (encrypted)`.
mkdir -p .ssh
write "a" ".ssh/id_rsa"
write "b" ".ssh/known_hosts"
dfm add -e .ssh/id_rsa
dfm add -e .ssh/known_hosts

# Mixed directory (one encrypted, one plain): folds without a marker; the
# encrypted member is still listed individually with `(encrypted)` under -e.
mkdir -p mixed
write "e" "mixed/enc.txt"
write "p" "mixed/plain.txt"
dfm add -e mixed/enc.txt
dfm add mixed/plain.txt

# --all: folded encrypted dir carries the marker; the plain member does not.
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "(encrypted)" <<<"$RES"
assert_succ grep -qF ".ssh/*" <<<"$RES"
assert_succ grep -qF -- "--  .ssh/*  (encrypted)" <<<"$RES"
# the mixed dir folds without the marker (not wholly encrypted)
assert_succ grep -qF "mixed/*" <<<"$RES"
assert_fail grep -qE "mixed/\\*.*encrypted" <<<"$RES"

# -e: encrypted files shown, markers present and right-aligned.
RES=$(dfm status -e 2>/dev/null)
assert_succ grep -qF ".ssh/*" <<<"$RES"
assert_succ grep -qF "(encrypted)" <<<"$RES"
assert_succ grep -qF "mixed/enc.txt" <<<"$RES"

# --short / --porcelain: clean, no marker.
RES=$(dfm status --short 2>/dev/null)
assert_fail grep -qF "(encrypted)" <<<"$RES"
RES=$(dfm status --porcelain 2>/dev/null)
assert_fail grep -qF "(encrypted)" <<<"$RES"

# An encrypted file that is also ignored shows both the pattern and the marker.
dfm ignore "mixed/enc.txt"
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF "(mixed/enc\\.txt)" <<<"$RES"
assert_succ grep -qF "(encrypted)" <<<"$RES"
