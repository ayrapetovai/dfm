dfm init dotfiles
mkdir -p dir1/a
mkdir -p dir2/b
mkdir -p dir3/b
mkdir not_ignored_dir
write "text" file.txt

dfm ignore dir1
dfm ignore dir2 dir3

! dfm status --all | grep -q 'dir1|dir2|dir3'

