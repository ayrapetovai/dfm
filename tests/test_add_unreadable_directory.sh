# `dfm add` on a directory with strict permissions (e.g. ~/.ssh chmod 000)
# must succeed with a warning instead of failing, and add nothing until the
# permissions are restored. Regression test for the permission-denied abort.

PASSWORD="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

mkdir -p .ssh
write "key material" .ssh/id_rsa
chmod 000 .ssh

dfm add .ssh 2>&1 | grep -q "skipping unreadable path"
assert_no_source "dot_ssh/id_rsa"
assert_no_source "dot_ssh/id_rsa.encrypted"

# restore permissions → the directory is added normally (force-encrypted)
chmod 700 .ssh
dfm add .ssh
assert_source "dot_ssh/id_rsa.encrypted"
