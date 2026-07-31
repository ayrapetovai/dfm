# purge must not fail when the state/config paths cannot be resolved
# (HOME unset, XDG_STATE_HOME unset). Nothing is deletable in that case,
# so the command must exit 0 and skip everything instead of erroring out.

dfm init dotfiles

assert -f "$XDG_CONFIG_HOME/dfm/config.toml"
assert -d "$XDG_STATE_HOME/dfm"
assert -d "$PWD/dotfiles"

CONFIG_DIR="$XDG_CONFIG_HOME"

# HOME and XDG_STATE_HOME unset -> state dir + state file paths unresolvable;
# HOME unset also makes the config path unresolvable and the source dir
# (which comes from the state file) unresolvable. purge must exit 0.
env -u HOME -u XDG_DATA_HOME -u XDG_CACHE_HOME -u XDG_STATE_HOME \
  XDG_CONFIG_HOME="$CONFIG_DIR" "$EXECUTABLE" -v 2 purge 2>stderr.txt

# nothing could be resolved, so nothing was deleted
assert -f "$CONFIG_DIR/dfm/config.toml"
assert -d "$XDG_STATE_HOME/dfm"
assert -d "$PWD/dotfiles"
