dfm init dotfiles

# create a file in the target directory that has no corresponding source file
write "unmanaged" unmanaged_file.txt

# add a managed file to have something in source
write "managed" managed.txt
dfm add managed.txt

# pull all: unmanaged file should be silently skipped
dfm pull

# postcondition: managed file is unchanged in source
#                unmanaged file is unchanged in target
assert -f managed.txt
assert "managed" = "$(cat managed.txt)"
assert -f unmanaged_file.txt
assert "unmanaged" = "$(cat unmanaged_file.txt)"
