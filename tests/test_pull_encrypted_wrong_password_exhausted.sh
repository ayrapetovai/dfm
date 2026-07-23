# Verify retry exhaustion: after the first attempt with a wrong password
# triggers InvalidPassword, the retry also fails. `obtain_password`
# must be called exactly twice (not three times).
#
# 1. Encrypt with a CORRECT password first
# 2. Switch to shell command that always returns WRONG and logs each call
# 3. Clear the log, run `dfm pull` — must fail
# 4. Password log must show exactly 2 calls (first try + retry)

PASSWORD="$(uuid)"

dfm init dotfiles

# Step 1: encrypt using the correct password
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"
write "content" secret.txt
dfm add --encrypt secret.txt

# Step 2: switch to always-wrong command with call logging
dfm config --set obtain_password_shell_command "bash -c 'echo called >> \$0/pw_log; echo -n WRONG_PASSWORD' \"$PWD\""

rm secret.txt

# Clear the log — we only care about pull calls
rm -f "$PWD/pw_log"

assert_fail dfm pull

# Must have been called exactly twice (first try + retry, then stopped)
CALLS=$(wc -l < "$PWD/pw_log")
assert "$CALLS" -eq 2

# Target file must NOT have been created
assert_fail test -f secret.txt
