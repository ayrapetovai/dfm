# `dfm config --default` prints the full default configuration in TOML format.
# The output is redirectable into the config file: it overrides the current
# content, stays a valid config, and restores default behavior. It works before
# any state exists, so a user can create a default config file with
# `dfm config --default > "$(dfm paths ...)"`.

# --- works without any state file (pre-init usage) ---
DEFAULTS=$(dfm config --default 2>/dev/null)
printf '%s\n' "$DEFAULTS" | grep -qF 'dot_prefix = "dot_"'
printf '%s\n' "$DEFAULTS" | grep -qF 'symlink_postfix = ".symlink"'
printf '%s\n' "$DEFAULTS" | grep -qF 'encrypted_postfix = ".encrypted"'
printf '%s\n' "$DEFAULTS" | grep -qF 'obtain_password_shell_command = ""'
printf '%s\n' "$DEFAULTS" | grep -qF 'merge_tool_command = "vimdiff {target} {source} {result}"'
printf '%s\n' "$DEFAULTS" | grep -qF 'diff_tool_command = "vimdiff -M {target} {source}"'
printf '%s\n' "$DEFAULTS" | grep -qF 'diff_all_tool_command_target = "diff -u --color=always {source} {target}"'
printf '%s\n' "$DEFAULTS" | grep -qF 'diff_all_tool_command_source = "diff -u --color=always {target} {source}"'
printf '%s\n' "$DEFAULTS" | grep -qF 'diff_editable_tool_command = "vimdiff {target} {source}"'
printf '%s\n' "$DEFAULTS" | grep -qF 'force_encryption_for'

dfm init dotfiles

# --- output is byte-identical to the config file `init` writes ---
CONFIG_PATH=$(dfm paths 2>/dev/null | sed -n 's/^Config: //p')
printf '%s\n' "$DEFAULTS" > defaults.toml
assert_succ diff -u "$CONFIG_PATH" defaults.toml

# --- redirecting --default over an existing config restores defaults ---
CFG="$PWD/mine.toml"
echo 'dot_prefix = "foo_"' >"$CFG"
dfm config --default 2>/dev/null >"$CFG"

# the file is valid again and reads back the default values
OUT=$(dfm -c "$CFG" config --get dot_prefix)
printf '%s\n' "$OUT" | grep -qF 'dot_'
dfm -c "$CFG" config --list >/dev/null

# a state-backed command still works off the redirected config
write "a" "f.txt"
dfm -c "$CFG" add f.txt
write "b" "f.txt"
dfm -c "$CFG" status --porcelain f.txt 2>/dev/null | grep -qF "f.txt"

# --- --default is exclusive with --get/--set/--list ---
run_fail dfm -c "$CFG" config --default --get dot_prefix
run_fail dfm -c "$CFG" config --default --set dot_prefix bar_
run_fail dfm -c "$CFG" config --default --list