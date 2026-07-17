dfm init dotfiles
write "$(uuid)" file.txt
dfm add file.txt
rm file.txt
dfm pull -s file.txt
assert -L file.txt

# must remove not only the file.txt in source dir
# also the corresponding symlink in target dir
dfm forget file.txt

assert_fail test -L file.txt
assert_fail test -f file.txt
assert_fail test -f "$PWD/dotfiles/file.txt"
