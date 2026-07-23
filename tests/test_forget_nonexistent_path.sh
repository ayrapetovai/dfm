# B2c — forget a non-existent path succeeds with "nothing to do"
dfm init dotfiles

# path doesn't exist, nothing to forget → exit 0
dfm forget /nonexistent/path
