# Road map

## Implement features:
- For dfm status add possibility to take several paths.
If several paths provided by user then 'status' command must show
statuses only of these files/directories the same way it shows statuses for ALL files/directories.
If flags provided along the paths then status must take these flags in to account.
Update README.md: add descriptions of this feature in to paragraph of status command.


## Default ignore list
Add the following records to the ignore_file at `dfm init`

```text
\.rustup/
\.cache
\.cargo
\.npm
\.state
\.local
```

