# The dfm config file is managed like any other dotfile:
# add by name, report modification, re-add, forget.

dfm init dotfiles

# the config file exists after init but is unmanaged -> ?? in status
RES=$(dfm status 2>/dev/null)
assert_succ grep -qF "??  .config/dfm/config.toml" <<<"$RES"

# add by directory name (the reported bug)
dfm add .config/dfm

assert_source "dot_config/dfm/config.toml"
assert "$(cat "$PWD/dotfiles/dot_config/dfm/config.toml")" = "$(cat "$PWD/.config/dfm/config.toml")"

# synchronized now
RES=$(dfm status --all 2>/dev/null)
assert_succ grep -qF -- "--  .config/dfm/config.toml" <<<"$RES"

# editing via `config --set` modifies the target side
dfm config --set diff_tool_command "diff -u {target} {source}"
RES=$(dfm status --short 2>/dev/null)
assert_succ grep -qF "M  .config/dfm/config.toml" <<<"$RES"

# re-add syncs the change into the source copy
dfm add .config/dfm/config.toml
assert_succ grep -qF 'diff_tool_command = "diff -u {target} {source}"' "$PWD/dotfiles/dot_config/dfm/config.toml"

# forget stops managing it: source copy removed, target kept
dfm forget .config/dfm/config.toml
assert_no_source "dot_config/dfm/config.toml"
assert -f "$PWD/.config/dfm/config.toml"
