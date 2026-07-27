1. The function 'calc_local_ignore_file' must depend on where state directory is located. This must fix that integraion tests change
file '~/.local/state/dfm/ignore_file' during the test run.
2. The temporary directory for merging regular and encrypted files must be creted in /tmp directory, not in source directory.
3. When pulling file taht was 'dfm ignore'd and that was pulled earlier, the corresponging target file must not be changed.
6. User microxdg library instead of hardcoding XDG paths.
7. launcher.sh must output debug log of test case it failes nad must not output debut logs if the case scrypt did not failed. This meens that
launcher must rerun test with TRACE set to '-x' specificaly for this failed scrypt.

