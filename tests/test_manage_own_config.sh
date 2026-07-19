dfm init dotfiles
assert true = "$(cat $PWD/.config/dfm/config.toml | grep -oP 'manage_symlinks = \K\w+')"

# suppose source directory contained a config file for dfm
mkdir -p "$PWD/dotfiles/dot_config/dfm"
echo '
dot_prefix = "dot_"
symlink_postfix = ".symlink"
encrypted_postfix = ".encrypted"
manage_symlinks = false
hooks = []
dotfiles_only = false
' > "$PWD/dotfiles/dot_config/dfm/config.toml"

dfm pull -f
# expecting that config file was copies to target folder
assert false = "$(cat $PWD/.config/dfm/config.toml | grep -oP 'manage_symlinks = \K\w+')"
