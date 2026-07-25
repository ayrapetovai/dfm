1. When encrypting or decrypting files provide promp with text "password for {filename_relative_target_directory}:".
2. Add flag --managed to 'status' subcomand, if the flag specifid list only managed filesystem objects.
3. Check that forced encryption encrypts files in 'add' command.
4. 'forget' unpulled files does not remove it, in it is encrypted (password prompted), or target is absend the --force must be used.
5. Check: when source files is removed by 'forget' commnd, its parrent directory must alsoe be removed if it is not source director and contained only this file.
6. Check if runing an external comand in shell to obtain a password is unsecure and reserch how to make it secure.
7. Make 'status' command not to return 1 when conflicts detected, all succesful checks by 'status' must dfm to return exit code 0;
8. Test pager of 'status' command.
9. Check why output of 'status' command, whre there is only one block of file info, still prints a new line.
