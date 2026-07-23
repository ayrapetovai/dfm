dfm init dotfiles

# ignore all .txt files by regex pattern
dfm ignore --patterns '\.txt$'

# try to add a .txt file (should be silently ignored)
write "content" notes.txt
dfm add notes.txt

# postcondition: .txt file was NOT copied to source
assert_no_source "notes.txt"

# try to add a .md file (should not be blocked)
write "content" readme.md
dfm add readme.md

# postcondition: .md file WAS copied to source
assert_source "readme.md"
