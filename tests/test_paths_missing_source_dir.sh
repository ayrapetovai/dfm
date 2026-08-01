# dfm paths must still print the configured paths even when the source
# directory does not exist.

dfm init dotfiles

# remove the source directory entirely
rm -rf "$PWD/dotfiles"

dfm paths > "paths_output.txt" 2>&1

assert -s "paths_output.txt"
assert_succ grep -q 'Source' "paths_output.txt"
assert_succ grep -q 'Target' "paths_output.txt"
assert_succ grep -q 'Config' "paths_output.txt"
assert_succ grep -q 'State' "paths_output.txt"
