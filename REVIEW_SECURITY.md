# dfm — Code & Integration Test Review

Review date: 2026-08-03 · Base: `df84fb4` (main, 2 commits ahead of origin).

Scope: `src/` (main, lib, crypt, 10 commands), `tests/` (167 shell tests + launcher),
`Cargo.toml`, `.github/workflows`, `README.md`, `context.txt`.


## B. Security — encrypted files and public uploads

The encryption feature is explicitly aimed at files (`.ssh`) that users may push to a public
git repo. Several properties undermine that goal:

1. **Encrypted archives leak file names and structure.** `crypt.rs:128-148` writes each
   enclosing directory as a **non-encrypted** directory entry and the file entry's inner name
   is the plaintext target-relative path. Uploading `dot_ssh/config.encrypted` to a public
   repo reveals that the user has `.ssh/config`, the directory layout, and (non-zero-size)
   file sizes. If the filename itself is sensitive, this is a leak.
2. **Weak key derivation.** The `zip` crate's AES-256 default KDF is PBKDF2-HMAC-SHA1 with
   1000 iterations. Offline brute-force of a weak passphrase on an uploaded archive is cheap
   with commodity hardware (e.g. `hashcat`). Consider a stronger, configurable KDF or at
   least documenting the risk.
3. **`read_to_string` for encrypted content** (`crypt.rs:150`) fails on non-UTF-8/binary
   files (e.g. an SSH key with unusual encoding, or any binary). `dfm add --encrypt` on a
   binary file errors out. Should stream bytes (`fs::read`/`io::copy`), not text.
4. **Password in memory, never zeroized.** The global `Mutex<Option<String>>` cache
   (`crypt.rs:19-35`) keeps the passphrase for the process lifetime with no zeroization.
   Low risk for a CLI, but worth a comment or `secrecy`-style handling.
5. **`obtain_password_shell_command` is stored plaintext in the config file.** Documented
   trade-off (keychain lookup is the recommended pattern), but the value may itself be a
   secret-bearing string; flag in the docs.
6. **Password trimming** (`crypt.rs:74-77`) `trim_end_matches(['\r','\n'])` is reasonable,
   but the KDF/wrong-password path could log a misleading "not found" — verify the error
   surface for wrong passwords on `add` (encrypt) is actionable.

Positive: the password is piped to `sh` **stdin** (`crypt.rs:54-63`), so it never appears in
`ps aux`; wrong-password retry clears the cache and re-prompts once (`crypt.rs:192-201`).

