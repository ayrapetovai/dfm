dfm init dotfiles

# pull a path that doesn't exist either as a regular source or as a .symlink file
# must fail (consistent with test_pull_bad_path.sh)
assert_fail dfm pull "$PWD/dotfiles/nonexistent_file.txt"

# pull all when there are no source files at all → should succeed with nothing to do
dfm pull
