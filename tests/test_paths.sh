dfm init dotfiles

# paths command must print config, state, source and target paths
dfm paths > "$PWD/paths_output.txt" 2>&1

assert -s "$PWD/paths_output.txt"
assert_succ grep -q 'Source' "$PWD/paths_output.txt"
assert_succ grep -q 'Target' "$PWD/paths_output.txt"
assert_succ grep -q 'Config' "$PWD/paths_output.txt"
assert_succ grep -q 'State' "$PWD/paths_output.txt"
