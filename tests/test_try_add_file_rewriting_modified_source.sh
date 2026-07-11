dfm init dotfiles
echo "content1" > file.txt
dfm add file.txt
echo "content2" > ./dotfiles/file.txt
dfm add file.txt && exit 1
test "content2" = "$(cat ./dotfiles/file.txt)" || exit 1
test "content1" = "$(cat file.txt)" || exit 1
