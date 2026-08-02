dfm init dotfiles
assert ".symlink" = "$(cat $PWD/.config/dfm/config.toml | grep -oP 'symlink_postfix = \"\K[^\"]+')"

# suppose source directory contained a config file for dfm
mkdir -p "$PWD/dotfiles/dot_config/dfm"
echo '
dot_prefix = "dot_"
symlink_postfix = ".ln"
encrypted_postfix = ".encrypted"
' > "$PWD/dotfiles/dot_config/dfm/config.toml"

dfm pull -f
# expecting that config file was copied to target folder
assert ".ln" = "$(cat $PWD/.config/dfm/config.toml | grep -oP 'symlink_postfix = \"\K[^\"]+')"
