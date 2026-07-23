# G1a — invalid regex pattern causes an error
dfm init dotfiles

assert_fail dfm ignore --patterns '['

assert_fail dfm ignore -p '(unclosed'
