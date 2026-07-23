dfm init dotfiles

# paths command must print config, state, source and target paths
dfm paths > "$PWD/paths_output.txt" 2>&1

assert -s "$PWD/paths_output.txt"
grep -q 'config' "$PWD/paths_output.txt" || { echo "Assertion failed: config not found in paths output"; exit 1; }
grep -q 'state' "$PWD/paths_output.txt" || { echo "Assertion failed: state not found in paths output"; exit 1; }
grep -q 'source' "$PWD/paths_output.txt" || { echo "Assertion failed: source not found in paths output"; exit 1; }
grep -q 'target' "$PWD/paths_output.txt" || { echo "Assertion failed: target not found in paths output"; exit 1; }
