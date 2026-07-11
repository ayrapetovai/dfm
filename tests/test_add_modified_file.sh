dfm init dotfiles
echo "content1" > file.txt
dfm add file.txt
echo "content2" > file.txt
dfm add file.txt
rm file.txt
dfm pull
test "content2" = "$(cat file.txt)" || exit 1
