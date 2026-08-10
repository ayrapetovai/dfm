# purge with a custom `-c` config path must remove only the config file itself;
# it must never delete the config file's parent directory and its other content.

CONTENT="$(uuid)"

dfm init dotfiles
write "$CONTENT" file.txt
dfm add file.txt

ALT="$PWD/altdir"
mkdir -p "$ALT"
write "$CONTENT" "$ALT/keepme.txt"
cp "$PWD/.config/dfm/config.toml" "$ALT/config.toml"

dfm -c "$ALT/config.toml" purge --force

# the config file itself is gone...
assert_fail test -f "$ALT/config.toml"

# ...but its parent directory and unrelated content survive
assert -d "$ALT"
assert -f "$ALT/keepme.txt"
assert_content_eq "$ALT/keepme.txt" "$CONTENT"

