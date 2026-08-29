# status --encrypted (-e): only managed entries whose `.encrypted` source has a
# managed plaintext target. -e overrides other restrictive filters and reports
# the encrypted set in every category. Orphaned `.encrypted` files are never shown.

PW="$(uuid)"

dfm init dotfiles

# Controlled non-encrypted file: must never appear with -e.
write "plain" "plainfile.txt"
dfm add plainfile.txt

# Encrypted managed file.
write "secret" "secret.txt"
dfm config --set obtain_password_shell_command "echo -n $PW"
dfm add -e secret.txt

# Orphaned .encrypted source file with no state entry: never shown by -e.
echo "orphan" > "dotfiles/orphan.encrypted"

# -e default report: only the encrypted entry, shown in the Up to date group.
RES=$(dfm status -e 2>/dev/null)
assert_succ grep -qF "secret.txt" <<<"$RES"
assert_fail grep -qF "plainfile.txt" <<<"$RES"
assert_fail grep -qF "orphan.encrypted" <<<"$RES"

# -e --porcelain / --short keep the unchanged, filtered format.
dfm status -e --porcelain 2>/dev/null | grep -qF -- "--	secret.txt"
dfm status -e --short 2>/dev/null | grep -qF -- "-- secret.txt"

# -e overrides other restrictive filters: --modified must still report the
# (up-to-date) encrypted entry.
RES=$(dfm status -e --modified 2>/dev/null)
assert_succ grep -qF "secret.txt" <<<"$RES"

# Target modified -> reported by -e in the Changes to add group.
write "changed" "secret.txt"
RES=$(dfm status -e 2>/dev/null)
assert_succ grep -qF "secret.txt" <<<"$RES"
assert_succ grep -qF "Changes to add" <<<"$RES"

# Target removed -> Unpulled: -e keeps the Unpulled block hidden in the default
# report, but --short still lists it.
rm secret.txt
RES=$(dfm status -e 2>/dev/null)
assert_fail grep -qF "Unpulled" <<<"$RES"
dfm status -e --short 2>/dev/null | grep -qF "!? secret.txt"
