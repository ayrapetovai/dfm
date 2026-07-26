2. The temporary directory for merging regular and encrypted files must be creted in /tmp directory, not in source directory.
3. When pulling file taht was 'dfm ignore'd and that was pulled earlier, the corresponging target file must not be changed.
5. In the output of 'status' command consider the list of ignored files: the info in parentecies is not aligned properly, the opening '(' of each
line must be right below the '(' of the previous line. This must hold if filename differ in length.

