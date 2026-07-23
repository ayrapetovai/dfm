# Verify password caching: within a single `dfm pull`, `obtain_password`
# is called exactly once even when decrypting multiple files.
#
# 1. Create two encrypted files
# 2. Add a shell command that logs each invocation to a counter file
# 3. Clear the log before pull
# 4. Pull — both files decrypt; password log must show exactly 1 call

PASSWORD="$(uuid)"
CONTENT_A="$(uuid)"
CONTENT_B="$(uuid)"

dfm init dotfiles

# Shell command that records each call to a log
dfm config --set obtain_password_shell_command "bash -c 'echo called >> \$0/pw_log; echo -n $PASSWORD' \"$PWD\""

mkdir -p sub
write "$CONTENT_A" a.txt
write "$CONTENT_B" sub/b.txt

dfm add --encrypt a.txt
dfm add --encrypt sub/b.txt

rm a.txt sub/b.txt
rmdir sub 2>/dev/null || true

# Clear the log — we only care about calls made during this pull
rm -f "$PWD/pw_log"

# One pull must decrypt both files
dfm pull

assert_content_eq "a.txt" "$CONTENT_A"
assert_content_eq "sub/b.txt" "$CONTENT_B"

# The password command was called exactly once (cached for the second file)
CALLS=$(wc -l < "$PWD/pw_log")
assert "$CALLS" -eq 1
