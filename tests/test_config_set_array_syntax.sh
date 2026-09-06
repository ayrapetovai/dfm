# `config --set` array syntax for the array parameter `force_encryption_for`:
# `add:` appends a regex, `rm:` removes every equal element (error if none),
# `rmi:` removes by 0-based index (error if invalid/out of range).
# A plain value on the array parameter, or a prefix on a scalar parameter,
# is an error with exit code 1.

dfm init dotfiles
CFG="$PWD/.config/dfm/config.toml"

# default array contains just the built-in rule
dfm config --get force_encryption_for | grep -qF '\.ssh'

# add: appends
dfm config --set force_encryption_for "add:\.git$"
dfm config --get force_encryption_for | grep -qF '\.git$'
dfm config --set force_encryption_for "add:\.txt$"
dfm config --get force_encryption_for | grep -qF '\.txt$'
# file stays a valid TOML array
grep -q '^force_encryption_for = \[' "$CFG"

# add: repairs a corrupted non-array value back into a proper array
BROKEN="$PWD/broken.toml"
dfm config --default >"$BROKEN"
sed -i 's|^force_encryption_for = .*|force_encryption_for = 42|' "$BROKEN"
dfm -c "$BROKEN" config --set force_encryption_for "add:\.cfg$" 2>/dev/null
grep -q '^force_encryption_for = \[' "$BROKEN"
dfm -c "$BROKEN" config --get force_encryption_for 2>/dev/null | grep -qF '\.cfg$'

# rm: removes every matching element
dfm config --set force_encryption_for "rm:\.git$"
OUT=$(dfm config --get force_encryption_for 2>/dev/null)
assert_succ grep -qF '\.ssh' <<<"$OUT"
assert_succ grep -qF '\.txt$' <<<"$OUT"
assert_fail grep -qF '\.git$' <<<"$OUT"

# rmi: removes by 0-based index: '\.ssh' is index 0 here
dfm config --set force_encryption_for "rmi:0"
OUT=$(dfm config --get force_encryption_for 2>/dev/null)
assert_fail grep -qF '\.ssh' <<<"$OUT"
assert_succ grep -qF '\.txt$' <<<"$OUT"
# rmi:0 again removes the remaining element
dfm config --set force_encryption_for "rmi:0"
OUT=$(dfm config --get force_encryption_for 2>/dev/null)
assert_fail grep -qF '\.txt$' <<<"$OUT"

# errors: exit code 1 with a message on stderr
run_fail dfm config --set force_encryption_for "rmi:5"
printf '%s\n' "$FAIL_OUTPUT" | grep -qF 'index out of range'
run_fail dfm config --set force_encryption_for "rmi:abc"
printf '%s\n' "$FAIL_OUTPUT" | grep -qF 'invalid index'
run_fail dfm config --set force_encryption_for "rm:\.nomatch$"
printf '%s\n' "$FAIL_OUTPUT" | grep -qF 'element not found'
run_fail dfm config --set force_encryption_for "add:["
printf '%s\n' "$FAIL_OUTPUT" | grep -qF 'invalid regex'
# add: with an empty element is rejected
run_fail dfm config --set force_encryption_for "add:"
printf '%s\n' "$FAIL_OUTPUT" | grep -qF 'must not be empty'
# a plain value on the array parameter is rejected
run_fail dfm config --set force_encryption_for "\.ssh"
printf '%s\n' "$FAIL_OUTPUT" | grep -qF 'is an array'
# prefixes on a scalar parameter are rejected
run_fail dfm config --set dot_prefix "add:x"
run_fail dfm config --set dot_prefix "rmi:0"
run_fail dfm config --set obtain_password_shell_command "rm:x"
# an unknown parameter is rejected before any syntax validation
run_fail dfm config --set force_encryption "add:abc"
printf '%s\n' "$FAIL_OUTPUT" | grep -qF 'is not found'
run_fail dfm config --set nonexistent plainvalue
# plain values on scalar parameters keep working
dfm config --set dot_prefix "foo_"
dfm config --get dot_prefix | grep -qF "foo_"

# the config stays valid and dfm still runs off it
dfm config --list >/dev/null
write "a" "plain.txt"
dfm add plain.txt
write "b" "plain.txt"
dfm status --porcelain plain.txt 2>/dev/null | grep -qF "plain.txt"

