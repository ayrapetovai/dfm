# `-c/--config` must actually point dfm at the alternate config file
# instead of being silently ignored.

dfm init dotfiles

ALT="$PWD/alt.toml"
echo 'dot_prefix = "cfg_"' >"$ALT"

# --get reads the alternate file when -c is given...
ALT_OUT=$(dfm -c "$ALT" config --get dot_prefix)
printf '%s\n' "$ALT_OUT" | grep -qF 'cfg_'

# ...and the default config otherwise.
DEFAULT_OUT=$(dfm config --get dot_prefix)
printf '%s\n' "$DEFAULT_OUT" | grep -qF 'dot_'
