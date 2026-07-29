11. Create file.txt, add it to source, ignore it, the remove 'dfm ignore -r file.txt' must remove file.txt from ignore_file in XDF_STATE_HOME/dfm.
12. dfm merge command with parameter (path) must run a merge tool, and do 3-way merge as thought there was a conflic BothModified.
13. Currently in the state file there are fields in each registration record: secs_since_epoch and nanos_since_epoch. They ocupy too much space, rename them to uts (unix time seconds) and utn (unix time nanos), the renaming in mapped struct in rust code is unessasery, just map thoese fields to the shorter names.

