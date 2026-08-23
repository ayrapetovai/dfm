dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n abc"
write "some text" .the_file

dfm add .the_file
rm .the_file

dfm ignore .the_file
RES=$(dfm status)
assert_fail grep -qF "the_file" <<<"$RES"
dfm pull
assert_fail test -f .the_file

write "text" .sencetive

dfm add -e .sencetive
rm .sencetive

dfm ignore .sencetive
dfm pull
RES=$(dfm status)
assert_fail grep -qF "sencetive" <<<"$RES"
assert_fail test -f .sencetive

