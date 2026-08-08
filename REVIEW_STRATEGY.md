Review source code and integrations tests for:
- best practices.
- duplicated code.
- robust error handling.
- logs do not hide the case of error.
- security, cryptographic stability, the encrypted files could be published to public repositories.
- ergonomics, all behavior is expected according to the best practices of CLI tools.
- safety, no information outside the source or target directories could be corrupted. No unmanaged information could be overwritten without explicit command.
- documents, license, manual, help subcommand.
- tests could not corrupt files outside the testing directory.

Write a result of the review into file REVIEW.md.

