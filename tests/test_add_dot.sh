# dfm add . must handle conflicts exactly like dfm add <file>: a conflict is
# a hard error (unless --force), and unmanaged files are added when no
# conflict exists.

dfm init dotfiles

# an unmanaged file in the target dir
write "unmanaged content" "newfile.txt"

# dfm add . must not fail
dfm add .

# verify it was added
assert_source "newfile.txt"

# now modify the source to create SourceModified state
echo "source modified content" > "$PWD/dotfiles/newfile.txt"

dfm status --short | grep -q "^ M newfile.txt$"

# dfm add . must FAIL on the managed M file, exactly like dfm add newfile.txt
assert_fail dfm add .
assert_fail dfm add newfile.txt

# the managed file must still show M
dfm status --short | grep -q "^ M newfile.txt$"

# an unmanaged file alongside a managed M file: add . aborts on the conflict
write "yet another" "extra.txt"
assert_fail dfm add .
assert_no_source "extra.txt"

# --force clears the conflict and adds both files
dfm add . --force

assert_source "extra.txt"
dfm status --short | grep -q "^-- newfile.txt$"

# clean up
rm extra.txt
