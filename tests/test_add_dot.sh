# dfm add . must succeed even when there are already-managed files
# in any state (clean, source-modified, both-modified, etc.)

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

# dfm add . must NOT fail just because a managed file has M state
dfm add .

# the managed file must still show M
dfm status --short | grep -q "^ M newfile.txt$"

# an unmanaged file alongside a managed M file
write "yet another" "extra.txt"

dfm add .

assert_source "extra.txt"
# managed file still M
dfm status --short | grep -q "^ M newfile.txt$"

# clean up
rm extra.txt
