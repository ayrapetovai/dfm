9. If file.txt was added, then removed from target, the ignored, then pulled with --force, then dfm must copy the file from source to target, and remove the file's regex from ignore_file.
10. If file.txt was added, then the pattern was removed from ignore_file by command 'dfm ignore -r file.txt' then next dfm pull must copy the file.txt from the source to target. The problem is that if the file contains some puncuation like '.' or '[', then in the ignore_file it becomes escaped with one slash \ , and if user forgets to pass exactly the same string, they will not match.
11. Create file.txt, add it to source, ignore it, the remove 'dfm ignore -r file.txt' must remove file.txt from ignore_file in XDF_STATE_HOME/dfm.
12. dfm merge command with parameter (path) must run a merge tool, and do 3-way merge as thought there was a conflic BothModified.
13. Currently in the state file there are fields in each registration record: secs_since_epoch and nanos_since_epoch. They ocupy too much space, rename them to uts (unix time seconds) and utn (unix time nanos), the renaming in mapped struct in rust code is unessasery, just map thoese fields to the shorter names.

