# When the wrong password is provided first, the user is re-prompted
# and given a second chance. This test simulates that with a shell
# command that returns WRONG on the first call and the CORRECT
# password on retry.
#
# 1. Create a helper script that alternates output
# 2. Encrypt a file with the correct password
# 3. Reset counter so first call returns wrong password
# 4. `dfm pull` — first attempt wrong → InvalidPassword → re-prompt →
#    second attempt correct → succeeds

PASSWORD="$(uuid)"

dfm init dotfiles

# Write a helper script that returns WRONG on first call,
# then PASSWORD on every subsequent call
cat > "$PWD/pw_getter.sh" << 'SCRIPT'
#!/bin/bash
COUNTER_FILE="$1"
CORRECT="$2"
COUNT=$(cat "$COUNTER_FILE" 2>/dev/null || echo 0)
echo $((COUNT + 1)) > "$COUNTER_FILE"
if [ "$COUNT" = "0" ]; then
  # First call: return wrong password
  echo -n "WRONG_PASSWORD"
else
  # Subsequent calls: return the correct password
  echo -n "$CORRECT"
fi
SCRIPT
chmod +x "$PWD/pw_getter.sh"

# Start with counter=1 so the first `obtain_password` (during add)
# returns the correct password
echo 1 > "$PWD/pw_counter"

dfm config --set obtain_password_shell_command "$PWD/pw_getter.sh $PWD/pw_counter $PASSWORD"

write "content" secret.txt
dfm add --encrypt secret.txt

# Reset counter to 0 — first pull attempt will get WRONG,
# then retry will get PASSWORD
echo 0 > "$PWD/pw_counter"

rm secret.txt
dfm pull

assert_content_eq "secret.txt" "content"
