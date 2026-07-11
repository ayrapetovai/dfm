dfm  init "dotfiles"
echo "content1" > file.txt
dfm add file.txt
rm file.txt
dfm pull
test "content1" = "$(cat file.txt)" || exit 1
