dfm init dotfiles
dfm config --set obtain_password_shell_command "echo -n abc"
write "some text" .the_file

dfm add .the_file
rm .the_file

dfm ignore .the_file
! dfm status | grep -q 'the_file'
dfm pull
assert_fail test -f .the_file

write "text" .sencetive

dfm add -e .sencetive
rm .sencetive

dfm ignore .sencetive
dfm pull
! dfm status | grep -q 'sencetive'
assert_fail test -f .sencetive

