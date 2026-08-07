# Add a symlink whose parent directory does not exist in the source tree.
# Regression: CreateSymlinkFilePointer used File::create without first creating
# the parent dir, so adding a symlink under a brand-new directory failed with
# "No such file or directory". The failure was masked when a sibling regular
# file was also added (that copy creates the dir), which is why the plain add
# tests never caught it.

dfm init dotfiles

# A symlink inside a subdirectory that does not exist in the source tree yet.
mkdir -p .config/systemd/user
mkdir -p .config/systemd/user/default.target.wants
echo "content" >".config/systemd/user/atuin.service"
ln -s "$PWD/.config/systemd/user/atuin.service" ".config/systemd/user/default.target.wants/atuin.service"

# Add only the symlink. Its source pointer file lives at
# dotfiles/dot_config/systemd/user/default.target.wants/atuin.service.symlink,
# whose parent dir is created by no other add task, so this used to fail.
dfm add .config/systemd/user/default.target.wants/atuin.service

# The pointer file must exist and point at the (absolute) pointee.
assert_source "dot_config/systemd/user/default.target.wants/atuin.service.symlink"
assert_content_eq "$PWD/dotfiles/dot_config/systemd/user/default.target.wants/atuin.service.symlink" "$PWD/.config/systemd/user/atuin.service"

# The add records the symlink in state; a follow-up status is clean.
dfm status >/dev/null 2>&1

