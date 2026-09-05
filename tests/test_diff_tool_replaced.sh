# Replacing the diff tool through `diff_tool_command` really switches what
# `dfm diff` runs: each configured tool's observable side effect appears only
# while that tool is configured, and after reconfiguring, the previous tool no
# longer runs.

CONTENT="$(uuid)"
MODIFIED="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

# The tool only runs when the pair diverges.
write "$MODIFIED" file.txt

# Tool A: records that it ran and the exact paths it was given.
cat > "$HOME/tool-a.sh" <<'TOOL'
#!/usr/bin/env bash
printf '%s\n' "$1" "$2" > "$HOME/tool-a-args.txt"
TOOL
chmod +x "$HOME/tool-a.sh"
dfm config --set diff_tool_command "$HOME/tool-a.sh {target} {source}"

dfm diff file.txt
assert -e "$HOME/tool-a-args.txt"
# The configured tool receives the resolved target and source paths, proving
# it really is `dfm diff` that launched it.
assert_succ grep -qxF "$HOME/file.txt" "$HOME/tool-a-args.txt"
assert_succ grep -qxF "$HOME/dotfiles/file.txt" "$HOME/tool-a-args.txt"

# Replacing the tool in config switches to the new one: the old tool's marker
# is not produced again, the new one's side effect is.
rm "$HOME/tool-a-args.txt"
cat > "$HOME/tool-b.sh" <<'TOOL'
#!/usr/bin/env bash
: > "$HOME/tool-b-ran"
TOOL
chmod +x "$HOME/tool-b.sh"
dfm config --set diff_tool_command "$HOME/tool-b.sh {target} {source}"

dfm diff file.txt
assert -e "$HOME/tool-b-ran"
assert ! -e "$HOME/tool-a-args.txt"