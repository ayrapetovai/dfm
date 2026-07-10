FILE_CONTENT="file content"
dfm  init "dotfiles"
echo "$FILE_CONTENT" > file.txt
dfm add file.txt
rm file.txt
dfm pull
test -f file.txt || exit 1
test "$FILE_CONTENT" = "$(cat file.txt)" || exit 2
