# dfm — Code & Integration Test Review
The encryption feature is explicitly aimed at files (`.ssh`) that users may push to a public
git repo. Several properties undermine that goal:

1. **Encrypted archives leak file names and structure.** `crypt.rs:128-148` writes each
   enclosing directory as a **non-encrypted** directory entry and the file entry's inner name
   is the plaintext target-relative path. Uploading `dot_ssh/config.encrypted` to a public
   repo reveals that the user has `.ssh/config`, the directory layout, and (non-zero-size)
   file sizes. If the filename itself is sensitive, this is a leak.

