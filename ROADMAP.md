Pain points:

6. Encryption UX — password prompt prints to stdout (mixing with program output), and the file needs an encryption password message is on stdout rather than stderr
8. No --help examples — subcommand help is sparse, just flag descriptions
1. No tab-completion — no shell completion script bundled
2. The walk through directories in all subcommands must not step in ignored directories. The traversing the collection of paths must must skip ignored directories. And the skipped directories must bot appear in progress information.
3. If dfm is launched with -v > 1 then no progress info must be shown.

Done:

5. Progress indicators for bulk operations — `traversing... N entries visited` during directory traversal and `processed X/Y files` during the analysis loop of `add`/`pull`/`forget`/`status`, written straight to stderr so they are visible at every verbosity level. Each update overwrites a single line in place (`\r`), and the line is erased when the operation finishes. Covered by `tests/test_progress_indicator.sh`.

