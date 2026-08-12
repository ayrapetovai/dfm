use crate::DfmError;
use log::debug;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::{Settings, file_path_relative_to, io_err};

// Password cache — ask only once per `dfm` process
//
// SECURITY NOTE: the passphrase lives in a plain `String` for the entire
// `dfm` process and is never zeroized on drop (Rust `String` does no explicit
// zeroization). This is an accepted trade-off for a short-lived CLI: the
// password is already resident in process memory while it is read from the
// subprocess stdout / tty and passed to the cipher provider, so the cache adds
// no new exposure beyond that. If this ever became a long-lived daemon or a
// library, migrate to `zeroize`/`secrecy` wrapping instead of `String`.

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

    debug!(
        "get password command is set to {:?}",
        settings.obtain_password_shell_command
    );

    let password = if let Some(get_password_command) =
        settings.obtain_password_shell_command.clone()
        && !get_password_command.is_empty()
    {
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
            return Err(DfmError::other(format!(
                "Error (return code {}): {}",
                output.status.code().unwrap_or(-1),
                stderr
            )));
        }
        // Most password providers (e.g. `security find-generic-password`,
        // `pass`) emit a trailing newline on stdout. Trim a single trailing
        // line terminator so the stored password matches what the user typed.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim_end_matches(['\r', '\n']);
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

// Encrypted blob format.
//
// Every `*.encrypted` file is a self-describing container: the password is
// stretched with Argon2id (memory-hard KDF), then the payload is authenticated-
// encrypted with XChaCha20-Poly1305. Layout (all integers little-endian):
//
//   magic            b"DFMENC\0"     8 bytes
//   version          u8              1 byte
//   header_len       u32             4 bytes
//   header           [header_len]    variable (v1) / fixed 52 bytes (v2)
//   ciphertext+tag   rest            16-byte Poly1305 tag appended
//
// header:
//   m_cost         u32  Argon2id memory cost in KiB
//   t_cost         u32  Argon2id time cost
//   p_cost         u32  Argon2id parallelism
//   salt           16 bytes
//   nonce          24 bytes (XChaCha20 nonce)
//
// plaintext (encrypted):
//   metadata_len   u32  length of the metadata section that follows
//   metadata             inner_name + file_mode + dir entries (see below)
//   content              the plaintext file bytes
//
// metadata:
//   inner_name     u16 len + UTF-8  (target-relative path of the plaintext)
//   file_mode      u32  unix permissions of the plaintext
//   dir_count      u16
//     per entry: dir_name u16 len + UTF-8 bytes, dir_mode u32
//
// Filenames, modes and directory structure are encrypted together with the
// content, so nothing about the payload is visible without the password.
//
// The header is the AEAD "additional data": a tampered header fails
// authentication, and a wrong password produces a tag mismatch that dfm uses
// to detect/retry. The KDF parameters travel in the header, so archives keep
// decrypting even when the code defaults change later.

const MAGIC: &[u8; 7] = b"DFMENC\x00";
const FORMAT_VERSION: u8 = 2;

// Argon2id cost parameters. Release builds use 64 MiB / 3 iterations / 4
// lanes so each password guess costs ~64 MiB of memory and measurable CPU.
// Debug builds (cargo build / cargo test) use weak params so the encryption
// tests run fast. The header records whichever params were used, so archives
// stay self-describing and decryptable across build profiles.
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

// Upper bounds accepted when reading KDF cost parameters from an encrypted
// blob's header. A crafted file committed to a shared repo must not be able
// to force a multi-GiB Argon2 allocation or a huge iteration count during
// `pull`. These caps are far above any legitimate value.
const MAX_KDF_M_COST_KIB: u32 = 16 * 1024 * 1024; // 16 GiB
const MAX_KDF_T_COST: u32 = 10;
const MAX_KDF_P_COST: u32 = 1024;

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
    let slice = data
        .get(pos..pos + 2)
        .ok_or_else(|| DfmError::InvalidData("encrypted header is truncated".into()))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32_le(data: &[u8], at: usize) -> Result<u32, DfmError> {
    let slice = data
        .get(at..at + 4)
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
        return Err(DfmError::InvalidData(
            "encrypted header is truncated".into(),
        ));
    }
    let s = std::str::from_utf8(&data[*pos..*pos + len])
        .map_err(|_| DfmError::InvalidData("non-UTF-8 string in encrypted header".into()))?
        .to_string();
    *pos += len;
    Ok(s)
}

/// Derive the symmetric key from the password exactly as the archive declares.
fn derive_key(
    password: &str,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<[u8; KEY_LEN], DfmError> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(m_cost, t_cost, p_cost, Some(KEY_LEN)).map_err(DfmError::other)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; KEY_LEN];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(DfmError::other)?;
    Ok(key)
}

/// Build a self-contained encrypted blob for `content`.
///
/// The public header holds only what the decryptor needs before the key
/// exists (KDF params, salt, nonce). Filename, file mode and directory
/// metadata are serialized into the plaintext and encrypted together with the
/// content, so nothing about the payload is visible without the password.
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

    // Metadata goes into the encrypted plaintext, prefixed by its length so
    // the decryptor knows where metadata ends and file content begins.
    let mut metadata: Vec<u8> = Vec::new();
    push_string(&mut metadata, inner_name);
    put_u32(&mut metadata, file_mode);
    put_u16(&mut metadata, dirs.len() as u16);
    for (dir, mode) in dirs {
        push_string(&mut metadata, &dir.to_string_lossy());
        put_u32(&mut metadata, *mode);
    }

    let mut plaintext = Vec::with_capacity(4 + metadata.len() + content.len());
    put_u32(&mut plaintext, metadata.len() as u32);
    plaintext.extend_from_slice(&metadata);
    plaintext.extend_from_slice(content);

    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(DfmError::other)?;
    let blob = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plaintext,
                aad: &header,
            },
        )
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
        return Err(DecryptError::Invalid(DfmError::InvalidData(
            "encrypted file is too short".into(),
        )));
    }
    if &data[..7] != MAGIC {
        return Err(DecryptError::Invalid(DfmError::InvalidData(
            "not a dfm encrypted file (bad magic)".into(),
        )));
    }
    let version = data[7];
    if version != FORMAT_VERSION {
        return Err(DecryptError::Invalid(DfmError::InvalidData(format!(
            "unsupported encrypted format version {}",
            version
        ))));
    }
    let header_len = read_u32_le(data, 8)? as usize;
    if 12 + header_len > data.len() {
        return Err(DecryptError::Invalid(DfmError::InvalidData(
            "encrypted header is truncated".into(),
        )));
    }
    let header = &data[12..12 + header_len];

    if header.len() < HEADER_FIXED {
        return Err(DecryptError::Invalid(DfmError::InvalidData(
            "encrypted header is truncated".into(),
        )));
    }
    let m_cost = read_u32_le(header, 0)?;
    let t_cost = read_u32_le(header, 4)?;
    let p_cost = read_u32_le(header, 8)?;
    if m_cost > MAX_KDF_M_COST_KIB || t_cost > MAX_KDF_T_COST || p_cost > MAX_KDF_P_COST {
        return Err(DecryptError::Invalid(DfmError::InvalidData(format!(
            "unreasonable KDF parameters in encrypted header: m_cost={}, t_cost={}, p_cost={}",
            m_cost, t_cost, p_cost
        ))));
    }
    let salt: [u8; SALT_LEN] = header[12..12 + SALT_LEN].try_into().unwrap();
    let nonce: [u8; NONCE_LEN] = header[12 + SALT_LEN..12 + SALT_LEN + NONCE_LEN]
        .try_into()
        .unwrap();

    let key = derive_key(password, &salt, m_cost, t_cost, p_cost)?;
    use chacha20poly1305::aead::{Aead, KeyInit, Payload};
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};
    let cipher = XChaCha20Poly1305::new_from_slice(&key).map_err(DfmError::other)?;
    let decrypted = match cipher.decrypt(
        XNonce::from_slice(&nonce),
        Payload {
            msg: &data[12 + header_len..],
            aad: header,
        },
    ) {
        Ok(c) => c,
        Err(_) => return Err(DecryptError::WrongPassword),
    };

    // The plaintext is: u32 metadata_len, then metadata, then the file bytes.
    let metadata_len = read_u32_le(&decrypted, 0)? as usize;
    if 4 + metadata_len > decrypted.len() {
        return Err(DecryptError::Invalid(DfmError::InvalidData(
            "encrypted metadata is truncated".into(),
        )));
    }
    let metadata = &decrypted[4..4 + metadata_len];
    let content = decrypted[4 + metadata_len..].to_vec();

    let mut pos = 0;
    let inner_name = take_string(metadata, &mut pos)?;
    let file_mode = read_u32_le(metadata, pos)?;
    pos += 4;
    let dir_count = read_u16_le(metadata, pos)? as usize;
    pos += 2;
    let mut dirs = Vec::with_capacity(dir_count);
    for _ in 0..dir_count {
        let name = take_string(metadata, &mut pos)?;
        let mode = read_u32_le(metadata, pos)?;
        pos += 4;
        dirs.push((PathBuf::from(name), mode));
    }
    Ok(Decrypted {
        content,
        file_mode,
        inner_name,
        dirs,
    })
}

/// Ancestors of a `target_dir`-relative path, shallowest first. For `a/b/c`
/// this yields `[a, a/b]`; a file directly in the target root yields none.
fn enclosing_dirs(rel: &Path) -> Vec<PathBuf> {
    if let Some(parent) = rel.parent() {
        parent
            .components()
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
    enclosing_dirs(inner_name)
        .into_iter()
        .filter_map(|dir_rel| {
            let dir_abs = target_dir_path.join(&dir_rel);
            fs::metadata(&dir_abs)
                .ok()
                .map(|m| (dir_rel, m.permissions().mode()))
        })
        .collect()
}

// Public writer/reader used by add / pull / merge / purge.

/// Print the "needs an encryption password" info line for the given target
/// file, exactly as `add` does before encrypting. Shared with the `diff`
/// command so a password prompt (prompting or decrypting) is always preceded
/// by the same "which file needs a password" info.
pub fn announce_encryption_password(target_file_path: &Path, target_dir: &Path) {
    let inner_name = file_path_relative_to(target_file_path, target_dir);
    let inner_name = inner_name.to_string_lossy();
    eprintln!("file {:?} needs an encryption password", inner_name);
}
/// Encrypt `target_file_path` into a new `*.encrypted` source file at
/// `source_file_path`, recording the file's permissions and the permissions of
/// every enclosing managed directory.
pub fn write_encrypted_file(
    settings: &Settings,
    target_file_path: &Path,
    source_file_path: &Path,
) -> Result<(), DfmError> {
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

    announce_encryption_password(target_file_path, &target_dir_path);

    let password = obtain_password(settings)?;
    let content = fs::read(target_file_path).map_err(|e| io_err(target_file_path, e))?;
    let blob = encrypt_bytes(
        &password,
        &content,
        target_file_permissions.mode(),
        &inner_name,
        &dirs,
    )?;

    let mut out =
        std::fs::File::create(source_file_path).map_err(|e| io_err(source_file_path, e))?;
    out.write_all(&blob)
        .map_err(|e| io_err(source_file_path, e))?;
    Ok(())
}

/// Decrypt an encrypted source file (dfm format) and write the plaintext to
/// `target_file_path`, recreating enclosing directory permissions recorded at
/// encrypt time.
pub fn read_encrypted_file(
    settings: &Settings,
    source_file_path: &Path,
    target_file_path: &Path,
) -> Result<(), DfmError> {
    // Ensure the target parent directory exists.
    if let Some(parent) = target_file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }

    let source_data = fs::read(source_file_path).map_err(|e| io_err(source_file_path, e))?;

    let target_root = PathBuf::from(&settings.target_dir);
    decrypt_blob_with_retry(settings, &source_data, source_file_path, &target_root, target_file_path)
}

/// Decrypt `source_data` (a dfm blob) into `target_file_path`, prompting for
/// the password and retrying once on a wrong password.
fn decrypt_blob_with_retry(
    settings: &Settings,
    source_data: &[u8],
    encrypted_path: &Path,
    target_root: &Path,
    target_file_path: &Path,
) -> Result<(), DfmError> {
    let decrypted = decrypt_with_retry(settings, source_data, encrypted_path)?;
    restore_target(target_root, target_file_path, &decrypted)
}

/// Decrypt a dfm blob, prompting for the password and retrying once on a wrong
/// password. Shared by every decrypt path (`read_encrypted_file`, standalone
/// `decrypt`, and `read_encrypted_bytes`).
fn decrypt_with_retry(
    settings: &Settings,
    source_data: &[u8],
    encrypted_path: &Path,
) -> Result<Decrypted, DfmError> {
    let mut already_retried = false;
    loop {
        let password = obtain_password(settings)?;

        match decrypt_bytes(source_data, &password) {
            Ok(decrypted) => return Ok(decrypted),
            Err(DecryptError::WrongPassword) if !already_retried => {
                clear_password_cache();
                eprintln!("wrong password for {:?}, please try again.", encrypted_path);
                already_retried = true;
                continue;
            }
            Err(DecryptError::WrongPassword) => {
                return Err(DfmError::other(format!(
                    "wrong password for encrypted file {:?}",
                    encrypted_path
                )));
            }
            Err(DecryptError::Invalid(e)) => {
                return Err(e);
            }
        }
    }
}

/// Decrypt an encrypted source file (dfm format) into memory, returning the
/// plaintext bytes and the recorded file mode. Used by `diff` to compare and
/// pipe the decrypted content to the diff tool without touching any file.
pub fn read_encrypted_bytes(
    settings: &Settings,
    source_file_path: &Path,
) -> Result<(Vec<u8>, u32), DfmError> {
    let source_data = fs::read(source_file_path).map_err(|e| io_err(source_file_path, e))?;
    let decrypted = decrypt_with_retry(settings, &source_data, source_file_path)?;
    Ok((decrypted.content, decrypted.file_mode))
}

/// Reject a directory entry recorded in an encrypted blob whose path could
/// escape the restore root: absolute paths, `..`-prefixed components and empty
/// entries (the latter would chmod the root itself). Defense-in-depth: this is
/// already gated by AEAD + a correct password, but a validly-decrypting blob
/// must never touch anything outside the target directory.
fn dir_rel_is_safe(dir_rel: &Path) -> bool {
    let mut saw_normal = false;
    for component in dir_rel.components() {
        match component {
            std::path::Component::Normal(_) => saw_normal = true,
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => return false,
        }
    }
    saw_normal
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
    target_file_path: &Path,
    decrypted: &Decrypted,
) -> Result<(), DfmError> {
    // Recreate enclosing directories with their recorded permissions (e.g. a
    // 0700 SSH directory that `create_dir_all` would otherwise reset).
    for (dir_rel, mode) in &decrypted.dirs {
        if !dir_rel_is_safe(dir_rel) {
            return Err(DfmError::InvalidData(format!(
                "encrypted file {:?} contains unsafe directory path {:?}",
                target_file_path, dir_rel
            )));
        }
        let dir_abs = target_root.join(dir_rel);
        fs::create_dir_all(&dir_abs).map_err(|e| io_err(&dir_abs, e))?;
        fs::set_permissions(&dir_abs, fs::Permissions::from_mode(*mode))
            .map_err(|e| io_err(&dir_abs, e))?;
    }

    if let Some(parent) = target_file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    fs::write(target_file_path, &decrypted.content).map_err(|e| io_err(target_file_path, e))?;
    fs::set_permissions(
        target_file_path,
        fs::Permissions::from_mode(decrypted.file_mode),
    )
    .map_err(|e| io_err(target_file_path, e))?;
    Ok(())
}

// Standalone `dfm encrypt` / `dfm decrypt` commands.

/// Encrypt `input_path` into `output_path` (a standalone file, not bound to a
/// target directory). The recorded inner name is the plain filename and no
/// directory-permission metadata is emitted.
pub fn encrypt_file_standalone(
    settings: &Settings,
    input_path: &PathBuf,
    output_path: &PathBuf,
) -> Result<(), DfmError> {
    let file_mode = fs::metadata(input_path)
        .map_err(|e| io_err(input_path, e))?
        .permissions()
        .mode();
    let content = fs::read(input_path).map_err(|e| io_err(input_path, e))?;
    let inner_name = input_path
        .file_name()
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
pub fn decrypt_file_standalone(
    settings: &Settings,
    input_path: &Path,
    output_path: &Path,
) -> Result<(), DfmError> {
    let source_data = fs::read(input_path).map_err(|e| io_err(input_path, e))?;

    let target_root = output_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    decrypt_blob_with_retry(settings, &source_data, input_path, &target_root, output_path)
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
        let dirs = vec![(PathBuf::from("private"), 0o700), (PathBuf::from("private/sub"), 0o710)];

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
        assert!(matches!(
            decrypt_bytes(&blob, "two"),
            Err(DecryptError::WrongPassword)
        ));
    }

    #[test]
    fn tampered_header_fails() {
        let blob = encrypt_bytes("pw", b"data", 0o644, "a", &[]).unwrap();
        // Flip a bit inside the header (in the header_len field region).
        let mut tampered = blob.clone();
        tampered[9] ^= 0x01;
        assert!(matches!(
            decrypt_bytes(&tampered, "pw"),
            Err(DecryptError::Invalid(_))
        ));
    }

    #[test]
    fn absurd_kdf_params_rejected() {
        let blob = encrypt_bytes("pw", b"data", 0o644, "a", &[]).unwrap();
        // m_cost lives at blob offset 12 (right after magic/version/header_len).
        // Set it to a multi-GiB value: decryption must fail fast with Invalid
        // rather than attempt the Argon2 allocation.
        let mut tampered = blob.clone();
        tampered[12..16].copy_from_slice(&(MAX_KDF_M_COST_KIB + 1).to_le_bytes());
        assert!(matches!(
            decrypt_bytes(&tampered, "pw"),
            Err(DecryptError::Invalid(_))
        ));

        // Same for an excessive t_cost (offset 16).
        let mut tampered = blob.clone();
        tampered[16..20].copy_from_slice(&(MAX_KDF_T_COST + 1).to_le_bytes());
        assert!(matches!(
            decrypt_bytes(&tampered, "pw"),
            Err(DecryptError::Invalid(_))
        ));
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let blob = encrypt_bytes("pw", b"data", 0o644, "a", &[]).unwrap();
        let mut tampered = blob.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(matches!(
            decrypt_bytes(&tampered, "pw"),
            Err(DecryptError::WrongPassword)
        ));
    }

    #[test]
    fn metadata_not_visible_in_blob() {
        let content = b"super-secret-content".to_vec();
        let dirs = vec![(PathBuf::from("private"), 0o700)];
        let blob = encrypt_bytes("pw", &content, 0o600, "private/secret.txt", &dirs).unwrap();

        // Neither the inner filename, its directory, nor the plaintext content
        // may appear in the encrypted blob without the password.
        for needle in ["private", "secret.txt", "super-secret-content"] {
            let found = blob.windows(needle.len()).any(|w| w == needle.as_bytes());
            assert!(
                !found,
                "metadata/content leaked: {:?} found in blob",
                needle
            );
        }

        // The metadata must be recoverable with the right password.
        let dec = decrypt_bytes(&blob, "pw").unwrap();
        assert_eq!(dec.inner_name, "private/secret.txt");
        assert_eq!(dec.dirs, dirs);
        assert_eq!(dec.content, content);
    }

    #[test]
    fn rejects_old_version() {
        // A v1 blob (metadata in the plaintext header) must be rejected.
        let blob = encrypt_bytes("pw", b"data", 0o644, "a", &[]).unwrap();
        let mut v1 = blob.clone();
        v1[7] = 1;
        assert!(matches!(
            decrypt_bytes(&v1, "pw"),
            Err(DecryptError::Invalid(_))
        ));
    }

    #[test]
    fn dir_rel_is_safe_rejects_escaping_paths() {
        assert!(dir_rel_is_safe(Path::new("private")));
        assert!(dir_rel_is_safe(Path::new("private/sub dir/deep")));

        for escaping in ["..", "../evil", "a/../../evil", "/abs/evil", "a//../b"] {
            assert!(
                !dir_rel_is_safe(Path::new(escaping)),
                "{:?} must be rejected",
                escaping
            );
        }
        // An empty entry would chmod the restore root itself, never allowed.
        assert!(!dir_rel_is_safe(Path::new(".")));
        assert!(!dir_rel_is_safe(Path::new("")));
    }
}


