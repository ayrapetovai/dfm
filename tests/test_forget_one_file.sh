dfm init dotfiles
write "text1" file1.txt
write "text2" file2.txt
mkdir d
write "text3" d/file3.txt

dfm add .
dfm forget file2.txt

assert_source "file1.txt"
assert_source "d/file3.txt"

grep -q "file1.txt" ./.local/state/dfm/state.toml
grep -q "d/file3.txt" ./.local/state/dfm/state.toml
assert_fail grep -q "file2.txt" ./.local/state/dfm/state.toml

