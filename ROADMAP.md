For each task of this list which supposues createion of a test file or changing code, check if the test file already exists.

7. Fill the 'Status' paragraph in README.md, now it says that 'status' is not impelemented, but is already.
8. Add flag --managed to 'status' subcomand, if the flag specifid list only managed filesystem objects.
9. Impelement feature: 'forget' unpulled files does not remove it, if it is encrypted (password prompted), or target is absent, the --force must be used.
11. Check if runing an external comand in shell to obtain a password is unsecure and reserch how to make it secure.
13. Check why output of 'status' command, when there is only one block of file info, still prints a new line.
14. Why the binary of dfm is that bloated even been stripped? Can we make it smaller?
15. Remove flag --merge from 'add' and from 'pull', clear the code of 'add' and 'pull' of merge logic.
