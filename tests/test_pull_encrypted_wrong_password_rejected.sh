# A consistently wrong password must cause `dfm pull` to fail after retry.
#
# 1. Encrypt a file with PASSWORD
# 2. Set shell command to a DIFFERENT (wrong) password
# 3. `dfm pull` must fail

PASSWORD="$(uuid)"
WRONG="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

write "secret content" secret.txt
dfm add --encrypt secret.txt

# Switch to the wrong password
dfm config --set obtain_password_shell_command "echo -n $WRONG"

rm secret.txt
assert_fail dfm pull
