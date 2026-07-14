dfm init dotfiles
FILES="$(ls -A)"
assert "$FILES" = "$(ls -A)"
dfm pull
assert "$FILES" = "$(ls -A)"
