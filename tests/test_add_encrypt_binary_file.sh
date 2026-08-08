# A binary (non-UTF-8) file must be encryptable and roundtrip intact via
# `dfm pull`. Regression for F1: the old writer rejected non-UTF-8 content.

PASSWORD="$(uuid)"

dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n $PASSWORD"

# binary content with invalid UTF-8 bytes (0x00, 0xff, 0x80)
printf 'bin\x00\xff\x80data' > bin.dat
dfm add --encrypt bin.dat

assert_source "bin.dat.encrypted"

# pull must recreate the exact bytes
rm bin.dat
dfm pull
cmp -s bin.dat <(printf 'bin\x00\xff\x80data')
