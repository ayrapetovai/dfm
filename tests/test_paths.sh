dfm init dotfiles

# paths command must print config, state, source and target paths
dfm paths > "$PWD/paths_output.txt" 2>&1

assert -s "$PWD/paths_output.txt"
grep -q 'Source' "$PWD/paths_output.txt" || { echo "Assertion failed: Source not found in paths output"; exit 1; }
grep -q 'Target' "$PWD/paths_output.txt" || { echo "Assertion failed: Target not found in paths output"; exit 1; }
grep -q 'Config' "$PWD/paths_output.txt" || { echo "Assertion failed: Config not found in paths output"; exit 1; }
grep -q 'State' "$PWD/paths_output.txt" || { echo "Assertion failed: State not found in paths output"; exit 1; }
