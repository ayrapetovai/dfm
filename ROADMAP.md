Pain points:

1. No --help examples — subcommand help is sparse, just flag descriptions
2. dfm fails to add directories and files that required permissions that user does not have. For example: the .ssh directory contains files with very strict permissions.
   Requirement: during execution of 'add', 'pull', and other commands if for some operation a special permission is required then dfm must ask user for the permissions and perform execution under with
   necessary rights.

