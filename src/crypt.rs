use std::process::{Command, Stdio};
use std::fs;
use std::io::{Cursor, Write};
use crate::DfmError;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use log::debug;
use zip::ZipArchive;

use crate::{Settings, file_path_relative_to, io_err};

// ---------------------------------------------------------------------------
// Password cache — ask only once per `dfm` process
//
// SECURITY NOTE: the passphrase lives in a plain `String` for the entire
// `dfm` process and is never zeroized on drop (Rust `String` does no explicit
// zeroization). This is an accepted trade-off for a short-lived CLI: the
// password is already resident in process memory while it is read from the
// subprocess stdout / tty and passed to the cipher provider, so the cache adds
// no new exposure beyond that. If this ever became a long-lived daemon or a
// library, migrate to `zeroize`/`secrecy` wrapping instead of `String`.
// ---------------------------------------------------------------------------

static PASSWORD_CACHE: Mutex<Option<String>> = Mutex::new(None);

fn get_cached_password() -> Option<String> {
    PASSWORD_CACHE.lock().ok().and_then(|c| c.clone())
}

fn set_cached_password(pw: String) {
    if let Ok(mut cache) = PASSWORD_CACHE.lock() {
        *cache = Some(pw);
    }
}

fn clear_password_cache() {
    if let Ok(mut cache) = PASSWORD_CACHE.lock() {
        *cache = None;
    }
}

/// Obtain the encryption/decryption password.  Uses the in-process cache
/// on subsequent calls so that the user is prompted only once per launch.
pub fn obtain_password(settings: &Settings) -> Result<String, DfmError> {
    // Check cache first
    if let Some(pw) = get_cached_password() {
        debug!("using cached password");
        return Ok(pw);
    }

    debug!("get password command is set to {:?}", settings.obtain_password_shell_command);

    let password = if let Some(get_password_command) = settings.obtain_password_shell_command.clone() &&
            !get_password_command.is_empty() {
        debug!("launching get password program");

        // Pipe the command to `sh` stdin instead of using `$SHELL -c` so the
        // command text does not appear in the process listing (`ps aux`).
        let mut child = Command::new("sh")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(get_password_command.as_bytes())?;
        }
        // stdin is dropped here → pipe closes → sh reads EOF and exits

        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DfmError::other(format!("Error (return code {}): {}", output.status.code().unwrap_or(-1), stderr)));
        }
        // Most password providers (e.g. `security find-generic-password`,
        // `pass`) emit a trailing newline on stdout. Trim a single trailing
        // line terminator so the stored password matches what the user typed.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout
            .trim_end_matches(['\r', '\n']);
        trimmed.to_string()
    } else {
        debug!("using default procedure to get password");
        eprint!(": ");
        let _ = std::io::stderr().flush();
        let pwd = default_read_password()?;
        eprintln!();
        pwd
    };

    // Cache for subsequent calls
    set_cached_password(password.clone());
    Ok(password)
}

// ---------------------------------------------------------------------------
// Encrypted blob format.
//
// Every `*.encrypted` file is a self-describing container: the password is
// stretched with Argon2id (memory-hard KDF), then the payload is authenticated-
// encrypted with XChaCha20-Poly1305. Layout (all integers little-endian):
//
//   magic            b"DFMENC\0"     8 bytes
//   version          u8              1 byte  (1)
//   header_len       u32             4 bytes
//   header           [header_len]    variable
//   ciphertext+tag   rest            16-byte Poly1305 tag appended
//
// header:
//   m_cost         u32  Argon2id memory cost in KiB
//   t_cost         u32  Argon2id time cost
//   p_cost         u32  Argon2id parallelism
//   salt           16 bytes
//   nonce          24 bytes (XChaCha20 nonce)
//   inner_name     u16 len + UTF-8  (target-relative path of the plaintext)
//   file_mode      u32  unix permissions of the plaintext
//   dir_count      u16
//     per entry: dir_name u16 len + UTF-8 bytes, dir_mode u32
//
// The header is the AEAD "additional data": a tampered header fails
// authentication, and a wrong password produces a tag mismatch that dfm uses
// to detect/retry. The KDF parameters travel in the header, so archives keep
// decrypting even when the code defaults change later.
// ---------------------------------------------------------------------------

const MAGIC: &[u8; 7] = b"DFMENC\x00";
const FORMAT_VERSION: u8 = 1;

// Argon2id cost parameters. 64 MiB / 3 iterations / 4 lanes: each password
// guess costs ~64 MiB of memory and measurable CPU, which makes brute-forcing
// a weak password far more expensive than the old fast-hash-and-zip scheme.
// Parameters are stored per-archive, so raising them later stays compatible.
// Argon2id parameters. Debug builds (cargo build / cargo test) use weak params
// so the encryption tests run fast; release builds use production-strength
// params. The header records whichever params were used, so files stay
// self-describing and decryptable across build profiles.
#[cfg(debug_assertions)]
const KDF_M_COST_KIB: u32 = 8192;
#[cfg(debug_assertions)]
const KDF_T_COST: u32 = 1;
#[cfg(debug_assertions)]
const KDF_P_COST: u32 = 1;

#[cfg(not(debug_assertions))]
const KDF_M_COST_KIB: u32 = 65536;
#[cfg(not(debug_assertions))]
const KDF_T_COST: u32 = 3;
#[cfg(not(debug_assertions))]
const KDF_P_COST: u32 = 4;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;

const HEADER_FIXED: usize = 12 + SALT_LEN + NONCE_LEN;

struct Decrypted {
    content: Vec<u8>,
    file_mode: u32,
    #[allow(dead_code)]
    inner_name: String,
    dirs: Vec<(PathBuf, u32)>,
}

#[derive(Debug)]
enum DecryptError {
    WrongPassword,
    Invalid(DfmError),
}

impl From<DfmError> for DecryptError {
    fn from(e: DfmError) -> Self {
        DecryptError::Invalid(e)
    }
}

fn read_u16_le(data: &[u8], pos: usize) -> Result<u16, DfmError> {
    let slice = data.get(pos..pos + 2)
        .ok_or_else(|| DfmError::InvalidData("encrypted header is truncated".into()))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(data: &[u8], at: usize) -> Result<u32, DfmError> {
    let slice = data.get(at..at + 4)
        .ok_or_else(|| DfmError::InvalidData("encrypted header is truncated".into()))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn put_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn put_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn push_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    put_u16(buf, bytes.len() as u16);
    buf.extend_from_slice(bytes);
}

fn take_string(data: &[u8], pos: &mut usize) -> Result<String, DfmError> {
    let len = read_u16_le(data, *pos)? as usize;
    *pos += 2;
    if *pos + len > data.len() {
        return Err(DfmError::InvalidData("encrypted header is truncated".into()));
    }
    let s = std::str::from_utf8(&data[*pos..*pos + len])
        .map_err(|_| DfmError::InvalidData("non-UTF-8 string in encrypted header".into()))?
        .to_string();
    *pos += len;
    Ok(s)
}

/// Derive the symmetric key from the password exactly as the archive declares.
fn derive_key(password: &str, salt: &[u8], m_cost: u32, t_cost: u32, p_cost: u32) -> Result<[u8; KEY_LEN], DfmError> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(m_cost, t_cost, p_cost, Some(KEY_LEN)).map_err(DfmError::other)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon.hash_password_into(password.as_bytes(), salt, &mut key).map_err(DfmError::other)?;
    Ok(key)
}

/// Build a self-contained encrypted blob for `content`.
pub fn encrypt_bytes(
    password: &str,
    content: &[u8],
    file_mode: u32,
    inner_name: &str,
    dirs: &[(PathBuf, u32)],
) -> Result<Vec<u8>, DfmError> {
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    use rand::RngCore;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut salt);
    rand::rng().fill_bytes(&mut nonce);

    let key = derive_key(password, &salt, KDF_M_COST_KIB, KDF_T_COST, KDF_P_COST)?;

    let mut header: Vec<u8> = Vec::new();
    put_u32(&mut header, KDF_M_COST_KIB);
    put_u32(&mut header, KDF_T_COST);
    put_u32(&mut header, KDF_P_COST);
    header.extend_from_slice(&salt);
    header.extend_from_slice(&nonce);
    push_string(&mut header, inner_name);
    put_u32(&mut header, file_mode);
    put_u16(&mut header, dirs.len() as u16);
    for (dir, mode) in dirs {
        push_string(&mut header, &dir.to_string_lossy());
        put_u32(&mut header, *mode);
    }

    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(DfmError::other)?;
    let blob = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: content, aad: &header })
        .map_err(|_| DfmError::other("encryption failed".to_string()))?;

    let mut out = Vec::with_capacity(12 + header.len() + blob.len());
    out.extend_from_slice(MAGIC);
    out.push(FORMAT_VERSION);
    put_u32(&mut out, header.len() as u32);
    out.extend_from_slice(&header);
    out.extend_from_slice(&blob);
    Ok(out)
}

/// Parse + decrypt an encrypted blob produced by `encrypt_bytes`.
fn decrypt_bytes(data: &[u8], password: &str) -> Result<Decrypted, DecryptError> {
    if data.len() < 12 {
        return Err(DecryptError::Invalid(DfmError::InvalidData("encrypted file is too short".into())));
    }
    if &data[..7] != MAGIC {
        return Err(DecryptError::Invalid(DfmError::InvalidData("not a dfm encrypted file (bad magic)".into())));
    }
    if data[7] != FORMAT_VERSION {
        return Err(DecryptError::Invalid(DfmError::InvalidData(format!(
            "unsupported encrypted format version {}", data[7]
        ))));
    }
    let header_len = read_u32_le(data, 8)? as usize;
    if 12 + header_len > data.len() {
        return Err(DecryptError::Invalid(DfmError::InvalidData("encrypted header is truncated".into())));
    }
    let header = &data[12..12 + header_len];

    if header.len() < HEADER_FIXED {
        return Err(DecryptError::Invalid(DfmError::InvalidData("encrypted header is truncated".into())));
    }
    let m_cost = read_u32_le(header, 0)?;
    let t_cost = read_u32_le(header, 4)?;
    let p_cost = read_u32_le(header, 8)?;
    let salt: [u8; SALT_LEN] = header[12..12 + SALT_LEN].try_into().unwrap();
    let nonce: [u8; NONCE_LEN] = header[12 + SALT_LEN..12 + SALT_LEN + NONCE_LEN].try_into().unwrap();

    let mut pos = HEADER_FIXED;
    let inner_name = take_string(header, &mut pos)?;
    let file_mode = read_u32_le(header, pos)?;
    pos += 4;
    let dir_count = read_u16_le(header, pos)? as usize;
    pos += 2;
    let mut dirs = Vec::with_capacity(dir_count);
    for _ in 0..dir_count {
        let name = take_string(header, &mut pos)?;
        let mode = read_u32_le(header, pos)?;
        pos += 4;
        dirs.push((PathBuf::from(name), mode));
    }

    let key = derive_key(password, &salt, m_cost, t_cost, p_cost)?;
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(DfmError::other)?;
    let content = match cipher.decrypt(
        XNonce::from_slice(&nonce),
        Payload { msg: &data[12 + header_len..], aad: header },
    ) {
        Ok(c) => c,
        Err(_) => return Err(DecryptError::WrongPassword),
    };

    Ok(Decrypted { content, file_mode, inner_name, dirs })
}

/// Ancestors of a `target_dir`-relative path, shallowest first. For `a/b/c`
/// this yields `[a, a/b]`; a file directly in the target root yields none.
fn enclosing_dirs(rel: &Path) -> Vec<PathBuf> {
    if let Some(parent) = rel.parent() {
        parent.components()
            .filter_map(|c| match c {
                std::path::Component::Normal(c) => Some(c),
                _ => None,
            })
            .scan(PathBuf::new(), |acc, c| {
                acc.push(c);
                Some(acc.clone())
            })
            .collect()
    } else {
        Vec::new()
    }
}

/// Enclosing-directory modes: `(target_rel_dir, mode)` for every component of
/// `inner_name`'s parent under `settings.target_dir`, so a decrypt can recreate
/// e.g. a 0700 SSH directory.
fn enclosing_dirs_with_modes(settings: &Settings, inner_name: &Path) -> Vec<(PathBuf, u32)> {
    let target_dir_path = PathBuf::from(&settings.target_dir);
    enclosing_dirs(inner_name).into_iter()
        .filter_map(|dir_rel| {
            let dir_abs = target_dir_path.join(&dir_rel);
            fs::metadata(&dir_abs)
                .ok()
                .map(|m| (dir_rel, m.permissions().mode()))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Public writer/reader used by add / pull / merge / purge.
// ---------------------------------------------------------------------------

/// Encrypt `target_file_path` into a new `*.encrypted` source file at
/// `source_file_path`, recording the file's permissions and the permissions of
/// every enclosing managed directory.
pub fn write_encrypted_file(settings: &Settings, target_file_path: &PathBuf, source_file_path: &PathBuf) -> Result<(), DfmError> {
    // Ensure the parent directory exists (important when the source path has
    // subdirectories).
    if let Some(parent) = source_file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }

    let target_file_permissions = fs::metadata(target_file_path)
        .map_err(|e| io_err(target_file_path, e))?
        .permissions();

    let target_dir_path = PathBuf::from(&settings.target_dir);
    let inner_name_p = file_path_relative_to(target_file_path, &target_dir_path);
    let inner_name = inner_name_p.to_string_lossy().into_owned();
    let dirs = enclosing_dirs_with_modes(settings, &inner_name_p);

    eprintln!("file {:?} needs an encryption password", inner_name);

    let password = obtain_password(settings)?;
    let content = fs::read(target_file_path).map_err(|e| io_err(target_file_path, e))?;
    let blob = encrypt_bytes(&password, &content, target_file_permissions.mode(), &inner_name, &dirs)?;

    let mut out = std::fs::File::create(source_file_path).map_err(|e| io_err(source_file_path, e))?;
    out.write_all(&blob).map_err(|e| io_err(source_file_path, e))?;
    Ok(())
}

/// Decrypt an encrypted source file (new dfm format or legacy zip archive)
/// and write the plaintext to `target_file_path`, recreating enclosing
/// directory permissions recorded at encrypt time.
pub fn read_encrypted_file(settings: &Settings, source_file_path: &PathBuf, target_file_path: &PathBuf) -> Result<(), DfmError> {
    // Ensure the target parent directory exists.
    if let Some(parent) = target_file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }

    let source_data = fs::read(source_file_path).map_err(|e| io_err(source_file_path, e))?;

    // Legacy sniff: a zip archive starts with the PK header.
    if source_data.len() >= 2 && &source_data[..2] == b"PK" {
        return read_zip_file(settings, source_file_path, target_file_path);
    }

    let target_root = PathBuf::from(&settings.target_dir);
    let mut already_retried = false;
    loop {
        let password = obtain_password(settings)?;

        match decrypt_bytes(&source_data, &password) {
            Ok(decrypted) => {
                restore_target(&target_root, target_file_path, &decrypted)?;
                return Ok(());
            }
            Err(DecryptError::WrongPassword) if !already_retried => {
                clear_password_cache();
                eprintln!("wrong password for {:?}, please try again.", source_file_path);
                already_retried = true;
                continue;
            }
            Err(DecryptError::WrongPassword) => {
                return Err(DfmError::other(format!(
                    "wrong password for encrypted file {:?}",
                    source_file_path
                )));
            }
            Err(DecryptError::Invalid(e)) => {
                return Err(e);
            }
        }
    }
}

/// Write the decrypted content to the target, restoring directory and file
/// permissions that were recorded at encrypt time.
///
/// Directory permissions are restored relative to the caller-supplied target
/// root. For the internal add/pull/merge/purge path this is
/// `settings.target_dir` (where the directories actually live); the standalone
/// `dfm decrypt` passes the output file's parent so dirs resolve relative to it.
fn restore_target(
    target_root: &Path,
    target_file_path: &PathBuf,
    decrypted: &Decrypted,
) -> Result<(), DfmError> {
    // Recreate enclosing directories with their recorded permissions (e.g. a
    // 0700 SSH directory that `create_dir_all` would otherwise reset).
    for (dir_rel, mode) in &decrypted.dirs {
        let dir_abs = target_root.join(dir_rel);
        fs::create_dir_all(&dir_abs).map_err(|e| io_err(&dir_abs, e))?;
        fs::set_permissions(&dir_abs, fs::Permissions::from_mode(*mode)).map_err(|e| io_err(&dir_abs, e))?;
    }

    if let Some(parent) = target_file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    fs::write(target_file_path, &decrypted.content).map_err(|e| io_err(target_file_path, e))?;
    fs::set_permissions(target_file_path, fs::Permissions::from_mode(decrypted.file_mode))
        .map_err(|e| io_err(target_file_path, e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Standalone `dfm encrypt` / `dfm decrypt` commands.
// ---------------------------------------------------------------------------

/// Encrypt `input_path` into `output_path` (a standalone file, not bound to a
/// target directory). The recorded inner name is the plain filename and no
/// directory-permission metadata is emitted.
pub fn encrypt_file_standalone(settings: &Settings, input_path: &PathBuf, output_path: &PathBuf) -> Result<(), DfmError> {
    let file_mode = fs::metadata(input_path).map_err(|e| io_err(input_path, e))?.permissions().mode();
    let content = fs::read(input_path).map_err(|e| io_err(input_path, e))?;
    let inner_name = input_path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| input_path.to_string_lossy().into_owned());

    eprintln!("file {:?} needs an encryption password", input_path);
    let password = obtain_password(settings)?;
    let blob = encrypt_bytes(&password, &content, file_mode, &inner_name, &[])?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let mut out = std::fs::File::create(output_path).map_err(|e| io_err(output_path, e))?;
    out.write_all(&blob).map_err(|e| io_err(output_path, e))?;
    Ok(())
}

/// Decrypt an encrypted standalone file `input_path` into `output_path`.
/// Directory-permission metadata (if any) is applied relative to the output
/// file's parent directory.
pub fn decrypt_file_standalone(settings: &Settings, input_path: &PathBuf, output_path: &PathBuf) -> Result<(), DfmError> {
    let source_data = fs::read(input_path).map_err(|e| io_err(input_path, e))?;

    if source_data.len() >= 2 && &source_data[..2] == b"PK" {
        return read_zip_file(settings, input_path, output_path);
    }

    let target_root = output_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();

    let mut already_retried = false;
    loop {
        let password = obtain_password(settings)?;

        match decrypt_bytes(&source_data, &password) {
            Ok(decrypted) => {
                restore_target(&target_root, output_path, &decrypted)?;
                return Ok(());
            }
            Err(DecryptError::WrongPassword) if !already_retried => {
                clear_password_cache();
                eprintln!("wrong password for {:?}, please try again.", input_path);
                already_retried = true;
                continue;
            }
            Err(DecryptError::WrongPassword) => {
                return Err(DfmError::other(format!(
                    "wrong password for encrypted file {:?}",
                    input_path
                )));
            }
            Err(DecryptError::Invalid(e)) => {
                return Err(e);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Legacy reader: AES-256 zip archives written by dfm < this change.
// Kept so existing `.encrypted` files in dotfiles repos keep decrypting.
// New writes use the Argon2id + XChaCha20-Poly1305 format above.
// ---------------------------------------------------------------------------

fn read_zip_file(settings: &Settings, source_zip_path: &PathBuf, target_file_path: &PathBuf) -> Result<(), DfmError> {
    let archive_bytes = fs::read(source_zip_path).map_err(|e| io_err(source_zip_path, e))?;

    // The legacy archive holds one encrypted file entry plus a non-encrypted
    // directory entry per enclosing directory (recorded by the old writer).
    // Directory entries recreate the target directories with their
    // permissions. A failed decrypt advances the reader, so re-open the
    // archive from the buffer each attempt; the wrong-password path re-prompts
    // once (the cache is cleared below).
    let mut already_retried = false;

    let target_dir_path = PathBuf::from(&settings.target_dir);
    eprintln!("file {:?} needs an encryption password", target_file_path);

    loop {
        let password = obtain_password(settings)?;

        let mut archive = match ZipArchive::new(Cursor::new(&archive_bytes)) {
            Ok(a) => a,
            Err(e) => return Err(DfmError::other(e)),
        };

        // Iterate all entries in index order. The directory entries come first
        // (added before the file entry), so their dirs exist before the file is
        // written. `by_index_decrypt` is safe on the plain directory entries —
        // the zip crate discards the password when the entry is not encrypted.
        let mut wrote_file = false;
        for i in 0..archive.len() {
            let mut zip_file = match archive.by_index_decrypt(i, password.as_bytes()) {
                Ok(f) => f,
                Err(zip::result::ZipError::InvalidPassword) if !already_retried => {
                    clear_password_cache();
                    eprintln!("wrong password for {:?}, please try again.", source_zip_path);
                    already_retried = true;
                    break;
                }
                Err(e) => return Err(DfmError::other(e)),
            };

            if zip_file.is_dir() {
                if let Some(rel) = zip_file.enclosed_name() {
                    let dir_abs = target_dir_path.join(&rel);
                    if let Some(mode) = zip_file.unix_mode() {
                        fs::create_dir_all(&dir_abs).map_err(|e| io_err(&dir_abs, e))?;
                        fs::set_permissions(&dir_abs, fs::Permissions::from_mode(mode))
                            .map_err(|e| io_err(&dir_abs, e))?;
                    }
                }
                continue;
            }

            let permissions = zip_file.unix_mode().map(fs::Permissions::from_mode);
            if let Some(parent) = target_file_path.parent() {
                fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
            }
            let mut output_file = std::fs::File::create(target_file_path)
                .map_err(|e| io_err(target_file_path, e))?;
            std::io::copy(&mut zip_file, &mut output_file).map_err(|e| io_err(target_file_path, e))?;
            if let Some(perms) = permissions {
                fs::set_permissions(target_file_path, perms).map_err(|e| io_err(target_file_path, e))?;
            }
            wrote_file = true;
            break;
        }

        if wrote_file {
            return Ok(());
        }
        if !already_retried {
            return Err(DfmError::other(format!(
                "encrypted archive has no file entry: {:?}",
                source_zip_path
            )));
        }
        // InvalidPassword retry: loop again; the password cache was cleared
        // above, so obtain_password re-prompts / re-reads.
    }
}

fn default_read_password() -> Result<String, DfmError> {
    let config = rpassword::ConfigBuilder::new()
         .password_feedback_mask('*')
         .build();

    rpassword::read_password_with_config(config)
        .map_err(|e| DfmError::other(format!("failed to read password: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let password = "correct horse battery staple";
        let content = b"the quick brown fox jumps over the lazy dog".to_vec();
        let mut dirs = Vec::new();
        dirs.push((PathBuf::from("private"), 0o700));
        dirs.push((PathBuf::from("private/sub"), 0o710));

        let blob = encrypt_bytes(password, &content, 0o600, "private/sub/f.conf", &dirs).unwrap();

        let dec = match decrypt_bytes(&blob, password) {
            Ok(d) => d,
            Err(e) => panic!("decrypt failed: {:?}", e),
        };
        assert_eq!(dec.content, content);
        assert_eq!(dec.file_mode, 0o600);
        assert_eq!(dec.inner_name, "private/sub/f.conf");
        assert_eq!(dec.dirs.len(), 2);
        assert_eq!(dec.dirs[0], (PathBuf::from("private"), 0o700));
        assert_eq!(dec.dirs[1], (PathBuf::from("private/sub"), 0o710));
    }

    #[test]
    fn wrong_password_fails() {
        let blob = encrypt_bytes("one", b"data", 0o644, "a", &[]).unwrap();
        assert!(matches!(decrypt_bytes(&blob, "two"), Err(DecryptError::WrongPassword)));
    }

    #[test]
    fn tampered_header_fails() {
        let blob = encrypt_bytes("pw", b"data", 0o644, "a", &[]).unwrap();
        // Flip a bit inside the header (in the header_len field region).
        let mut tampered = blob.clone();
        tampered[9] ^= 0x01;
        assert!(matches!(decrypt_bytes(&tampered, "pw"), Err(DecryptError::Invalid(_))));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let blob = encrypt_bytes("pw", b"data", 0o644, "a", &[]).unwrap();
        let mut tampered = blob.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(matches!(decrypt_bytes(&tampered, "pw"), Err(DecryptError::WrongPassword)));
    }
}