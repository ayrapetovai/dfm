# source/target dirs live in the state file, so removing the config file
# must not break state-dependent commands.

dfm init dotfiles

# the config file is deleted below; keep it out of the status report
dfm ignore .config/dfm

write "content" "file.txt"
dfm add file.txt
assert_source "file.txt"

# remove the dfm config directory entirely
rm -rf "$XDG_CONFIG_HOME/dfm"

# status still resolves source/target from state
dfm status 2>/dev/null | assert_succ grep -q "All up-to-date"
dfm status --all 2>/dev/null | assert_succ grep -qF -- "--  file.txt"

# pull still works
rm -f file.txt
dfm pull
assert -f "$PWD/file.txt"
assert_content_eq "$PWD/file.txt" "content"
