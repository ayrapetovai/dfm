use crate::DfmError;
use log::{debug, warn};
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use chacha20poly1305::XChaCha20Poly1305;
use zeroize::{Zeroize, Zeroizing};

use crate::{Settings, calc_working_dir_paths_unchecked, file_path_relative_to, io_err};

// Password cache — ask only once per `dfm` process
//
// The cache holds the passphrase for the whole `dfm` process, so it is wrapped
// in `Zeroizing`: every copy (cache, prompt result, subprocess stdout) is wiped
// on drop instead of being left in freed heap memory.

type CachedPassword = Zeroizing<String>;

static PASSWORD_CACHE: Mutex<Option<CachedPassword>> = Mutex::new(None);

fn get_cached_password() -> Option<CachedPassword> {
    PASSWORD_CACHE.lock().ok().and_then(|c| c.clone())
}

fn set_cached_password(password: CachedPassword) {
    if let Ok(mut cache) = PASSWORD_CACHE.lock() {
        *cache = Some(password);
    }
}

fn clear_password_cache() {
    if let Ok(mut cache) = PASSWORD_CACHE.lock() {
        *cache = None;
    }
}

/// Obtain the encryption/decryption password.  Uses the in-process cache
/// on subsequent calls so that the user is prompted only once per launch.
///
/// An empty password is refused: it would derive a perfectly valid key and
/// produce an archive that looks protected but is not.
pub fn obtain_password(settings: &Settings) -> Result<CachedPassword, DfmError> {
    // Check cache first
    if let Some(password) = get_cached_password() {
        debug!("using cached password");
        return Ok(password);
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
        // The raw provider output is wiped before it is dropped.
        let mut raw_stdout = Zeroizing::new(output.stdout);
        let stdout = String::from_utf8_lossy(&raw_stdout);
        let trimmed = stdout.trim_end_matches(['\r', '\n']);
        let password = Zeroizing::new(trimmed.to_string());
        raw_stdout.zeroize();
        password
    } else {
        debug!("using default procedure to get password");
        eprint!(": ");
        let _ = std::io::stderr().flush();
        let password = Zeroizing::new(default_read_password()?);
        eprintln!();
        password
    };

    // Checked without modifying the password: leading/trailing spaces are
    // legitimate password characters, but a provider that yields only
    // whitespace has failed rather than supplied one.
    if password.trim().is_empty() {
        return Err(DfmError::InvalidInput(
            "the password must not be empty or whitespace".into(),
        ));
    }

    // Cache for subsequent calls
    set_cached_password(password.clone());
    Ok(password)
}

// Encrypted blob format (v3, streaming).
//
// Every `*.encrypted` file is a self-describing container: the password is
// stretched with Argon2id (memory-hard KDF), then the payload is authenticated-
// encrypted with XChaCha20-Poly1305 in fixed-size chunks, so encryption and
// decryption never hold more than one chunk of plaintext/ciphertext in RAM
// regardless of file size. Layout (all integers little-endian):
//
//   magic            b"DFMENC\0"     8 bytes
//   version          u8              1 byte (3)
//   header_len       u32             4 bytes (fixed 64 for v3)
//   header           64 bytes
//   chunk*           sequence of sealed chunks until end of file
//
// header:
//   m_cost           u32  Argon2id memory cost in KiB
//   t_cost           u32  Argon2id time cost
//   p_cost           u32  Argon2id parallelism
//   salt             16 bytes
//   base_nonce       24 bytes
//   chunk_size       u32  plaintext bytes per chunk (bounded on read)
//   plaintext_len    u64  total plaintext length (metadata + content)
//
// chunk i:
//   ct_i             chunk_size bytes of ciphertext (last chunk shorter)
//   tag_i            16-byte Poly1305 tag over ct_i
//
// Per-chunk nonce: base_nonce with its last 8 bytes replaced by a 7-byte
// big-endian chunk counter followed by a final-chunk flag byte. The per-chunk
// AAD is header || u64(i) || flag. This binds every chunk to its position:
// reordered, duplicated, replayed-as-final or truncated chunks fail
// authentication instead of silently decrypting.
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
// The header is part of every chunk's AAD: a tampered header fails
// authentication, and a wrong password produces a tag mismatch on the first
// chunk that dfm uses to detect/retry. The KDF parameters travel in the
// header, so archives keep decrypting even when the code defaults change.
//
// KNOWN METADATA LEAK: `plaintext_len` and `chunk_size` are stored in
// cleartext, so anyone holding an archive learns its exact plaintext size and
// chunk count. This is structural, not incidental: the final-chunk flag feeds
// both the per-chunk nonce and the AAD, so the total length must be known
// before the last chunk can be authenticated, which is what makes single-pass
// streaming decryption possible. Length is therefore the one property that
// cannot be hidden without giving up streaming.

const MAGIC: &[u8; 7] = b"DFMENC\x00";
const FORMAT_VERSION: u8 = 3;

// Argon2id cost parameters. They are the same for every build profile: the
// parameters are written into the header, so an archive made by a debug build
// would stay weak forever and nothing in the container marks it as such. Tests
// pay the real cost (~0.1 s per operation) rather than silently producing
// archives that a release build would never create.
const KDF_M_COST_KIB: u32 = 65536;
const KDF_T_COST: u32 = 3;
const KDF_P_COST: u32 = 4;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;

// Streaming chunk size: the amount of plaintext sealed per AEAD record. Peak
// memory during encrypt/decrypt is O(chunk_size) + Argon2, not O(file).
const CHUNK_SIZE: u32 = 64 * 1024;

// Bounds accepted when reading `chunk_size` from an encrypted header. A
// crafted file must not force a huge allocation nor a pathological number of
// tiny records.
const MIN_CHUNK_SIZE: u32 = 256;
const MAX_CHUNK_SIZE: u32 = 8 * 1024 * 1024;

// The per-chunk nonce carries a 7-byte (56-bit) counter; refuse to encrypt
// beyond it. With 64 KiB chunks this is a 4 EiB ceiling — unreachable.
const MAX_STREAM_CHUNKS: u64 = 1 << 56;

// Upper bounds accepted when reading KDF cost parameters from an encrypted
// blob's header. A crafted file committed to a shared repo must not be able
// to force a multi-GiB Argon2 allocation or a huge iteration count during
// `pull`. These caps are far above any legitimate value.
const MAX_KDF_M_COST_KIB: u32 = 16 * 1024 * 1024; // 16 GiB
const MAX_KDF_T_COST: u32 = 10;
const MAX_KDF_P_COST: u32 = 1024;

// Lower bounds, the mirror image of the caps above. A blob below them was
// written with a cost too low to resist guessing (older `dfm` builds keyed the
// parameters to the build profile, so every archive they produced carries
// 8 MiB / t1 / p1 and falls out of a small wordlist in minutes). Such a blob is
// still readable — refusing would lock the owner out of their own files — but
// the weakness is reported.
//
// Calibration: the floors are set *just below* what this build writes, so the
// in-force parameters always pass and the legacy weak profile always trips the
// warning. `p_cost` is floored at 1 because a single lane is a legitimate
// low-memory choice, not a weakness; `t_cost` is floored at 2 because one
// iteration removes the time-memory cost the KDF exists to impose.
const MIN_KDF_M_COST_KIB: u32 = 32 * 1024; // 32 MiB
const MIN_KDF_T_COST: u32 = 2;
const MIN_KDF_P_COST: u32 = 1;

/// The reduced cost older builds keyed to their debug profile, kept as a named
/// value so the warning floor and the legacy-blob test agree on it.
const LEGACY_DEBUG_KDF: KdfCost = KdfCost {
    m_cost_kib: 8192,
    t_cost: 1,
    p_cost: 1,
};

// header: m(4) t(4) p(4) salt(16) base_nonce(24) chunk_size(4) plaintext_len(8)
const HEADER_FIXED: usize = 12 + SALT_LEN + NONCE_LEN + 4 + 8;

/// Metadata recovered from the first plaintext chunk; everything except the
/// file bytes (which stream through separately).
struct PlainMeta {
    file_mode: u32,
    #[allow(dead_code)] // round-trips through the blob; surfaced in tests
    inner_name: String,
    dirs: Vec<(PathBuf, u32)>,
}

/// Full in-memory decryption result — only the test wrapper materializes it.
#[cfg(test)]
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

fn read_u64_le(data: &[u8], at: usize) -> Result<u64, DfmError> {
    let slice = data
        .get(at..at + 8)
        .ok_or_else(|| DfmError::InvalidData("encrypted header is truncated".into()))?;
    let mut b = [0u8; 8];
    b.copy_from_slice(slice);
    Ok(u64::from_le_bytes(b))
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
/// Derive the 32-byte content key. The result is `Zeroizing` because the key
/// is the value an attacker actually wants: wiping the password while leaving
/// the key it produced in freed stack memory would defeat the point.
fn derive_key(
    password: &str,
    salt: &[u8],
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<Zeroizing<[u8; KEY_LEN]>, DfmError> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(m_cost, t_cost, p_cost, Some(KEY_LEN)).map_err(DfmError::other)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(DfmError::other)?;
    Ok(key)
}

/// Serialize the metadata section (including its leading length prefix).
fn serialize_metadata(inner_name: &str, file_mode: u32, dirs: &[(PathBuf, u32)]) -> Vec<u8> {
    let mut metadata: Vec<u8> = Vec::new();
    push_string(&mut metadata, inner_name);
    put_u32(&mut metadata, file_mode);
    put_u16(&mut metadata, dirs.len() as u16);
    for (dir, mode) in dirs {
        push_string(&mut metadata, &dir.to_string_lossy());
        put_u32(&mut metadata, *mode);
    }
    let mut prefix: Vec<u8> = Vec::with_capacity(4 + metadata.len());
    put_u32(&mut prefix, metadata.len() as u32);
    prefix.extend_from_slice(&metadata);
    prefix
}

/// Per-chunk nonce: base_nonce with its last 8 bytes replaced by the 7-byte
/// big-endian chunk counter and a final-chunk flag byte.
fn stream_nonce(base_nonce: &[u8; NONCE_LEN], index: u64, last: bool) -> [u8; NONCE_LEN] {
    debug_assert!(index < MAX_STREAM_CHUNKS);
    let ctr = index.to_be_bytes();
    let mut nonce = *base_nonce;
    nonce[16..NONCE_LEN - 1].copy_from_slice(&ctr[1..]);
    nonce[NONCE_LEN - 1] = u8::from(last);
    nonce
}

/// Per-chunk additional authenticated data: header, chunk position, final flag.
fn chunk_aad(header: &[u8], index: u64, last: bool) -> Vec<u8> {
    let mut aad = Vec::with_capacity(header.len() + 9);
    aad.extend_from_slice(header);
    aad.extend_from_slice(&index.to_le_bytes());
    aad.push(u8::from(last));
    aad
}

/// Encrypt `input` (exactly `content_len` bytes) into a v3 container written
/// to `out`. The metadata prefix is prepended to the plaintext stream, so it
/// is authenticated together with the content. Peak memory is one chunk.
///
/// The caller must guarantee that `input` really yields `content_len` bytes;
/// a shorter stream is a hard error (the header's declared length is part of
/// every chunk's authentication data).
fn encrypt_stream(
    password: &str,
    input: &mut impl Read,
    content_len: u64,
    metadata_prefix: &[u8],
    out: &mut impl Write,
) -> Result<(), DfmError> {
    let mut salt = [0u8; SALT_LEN];
    let mut base_nonce = [0u8; NONCE_LEN];
    {
        use rand::RngCore;
        rand::rng().fill_bytes(&mut salt);
        rand::rng().fill_bytes(&mut base_nonce);
    }
    encrypt_stream_seeded(
        password,
        input,
        content_len,
        metadata_prefix,
        out,
        KdfCost {
            m_cost_kib: KDF_M_COST_KIB,
            t_cost: KDF_T_COST,
            p_cost: KDF_P_COST,
        },
        &salt,
        &base_nonce,
    )
}

/// Argon2id cost parameters, carried together so the encrypt path and its
/// tests agree on one value set.
#[derive(Clone, Copy)]
struct KdfCost {
    m_cost_kib: u32,
    t_cost: u32,
    p_cost: u32,
}

/// The body of [`encrypt_stream`], with the cost parameters and the random
/// material supplied by the caller. Production callers always pass
/// [`KDF_M_COST_KIB`] and friends plus fresh randomness; only tests pass a
/// reduced cost, to build a legacy blob that still authenticates.
#[allow(clippy::too_many_arguments)]
fn encrypt_stream_seeded(
    password: &str,
    input: &mut impl Read,
    content_len: u64,
    metadata_prefix: &[u8],
    out: &mut impl Write,
    cost: KdfCost,
    salt: &[u8; SALT_LEN],
    base_nonce: &[u8; NONCE_LEN],
) -> Result<(), DfmError> {
    use chacha20poly1305::aead::AeadInPlace;
    use chacha20poly1305::aead::KeyInit;
    use chacha20poly1305::{XChaCha20Poly1305, XNonce};

    if metadata_prefix.len() > CHUNK_SIZE as usize {
        return Err(DfmError::InvalidData(format!(
            "encrypted metadata of {} bytes does not fit into one {}-byte chunk",
            metadata_prefix.len(),
            CHUNK_SIZE
        )));
    }
    let plaintext_len = metadata_prefix.len() as u64 + content_len;

    let key = derive_key(password, salt, cost.m_cost_kib, cost.t_cost, cost.p_cost)?;

    let mut header: Vec<u8> = Vec::with_capacity(HEADER_FIXED);
    put_u32(&mut header, cost.m_cost_kib);
    put_u32(&mut header, cost.t_cost);
    put_u32(&mut header, cost.p_cost);
    header.extend_from_slice(salt);
    header.extend_from_slice(base_nonce);
    put_u32(&mut header, CHUNK_SIZE);
    header.extend_from_slice(&plaintext_len.to_le_bytes());

    out.write_all(MAGIC)?;
    out.write_all(&[FORMAT_VERSION])?;
    out.write_all(&(header.len() as u32).to_le_bytes())?;
    out.write_all(&header)?;

    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref()).map_err(DfmError::other)?;

    let mut buf = vec![0u8; CHUNK_SIZE as usize];
    let mut consumed = 0u64;
    let mut index = 0u64;
    loop {
        // Fill exactly `want` bytes: the declared total length decides where
        // the final (flagged) chunk is, not a short read.
        let want = std::cmp::min(buf.len() as u64, plaintext_len - consumed) as usize;
        let mut off = 0usize;
        if index == 0 {
            buf[..metadata_prefix.len()].copy_from_slice(metadata_prefix);
            off = metadata_prefix.len();
        }
        while off < want {
            let r = input
                .read(&mut buf[off..want])
                .map_err(|e| io_err(Path::new("<input>"), e))?;
            if r == 0 {
                return Err(DfmError::InvalidData(
                    "input ended before its declared length".into(),
                ));
            }
            off += r;
        }
        consumed += want as u64;
        let last = consumed == plaintext_len;

        let nonce = stream_nonce(base_nonce, index, last);
        let aad = chunk_aad(&header, index, last);
        let tag = cipher
            .encrypt_in_place_detached(XNonce::from_slice(&nonce), &aad, &mut buf[..want])
            .map_err(|_| DfmError::other("encryption failed".to_string()))?;

        out.write_all(&buf[..want])?;
        out.write_all(tag.as_slice())?;

        if last {
            break;
        }
        index += 1;
        if index >= MAX_STREAM_CHUNKS {
            return Err(DfmError::InvalidData(
                "stream is too long to encrypt".into(),
            ));
        }
    }
    Ok(())
}

/// In-memory convenience wrapper over [`encrypt_stream`] (used by tests and
/// tooling); real file paths go through the streaming callers directly.
pub fn encrypt_bytes(
    password: &str,
    content: &[u8],
    file_mode: u32,
    inner_name: &str,
    dirs: &[(PathBuf, u32)],
) -> Result<Vec<u8>, DfmError> {
    let prefix = serialize_metadata(inner_name, file_mode, dirs);
    let mut blob = Vec::with_capacity(12 + HEADER_FIXED + prefix.len() + content.len());
    encrypt_stream(
        password,
        &mut std::io::Cursor::new(content),
        content.len() as u64,
        &prefix,
        &mut blob,
    )?;
    Ok(blob)
}

/// Parsed, validated container header. The raw bytes are kept because every
/// chunk authenticates them as AAD.
struct ContainerHeader {
    raw: Vec<u8>,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
    salt: [u8; SALT_LEN],
    base_nonce: [u8; NONCE_LEN],
    chunk_size: usize,
    plaintext_len: u64,
}

fn truncated() -> DfmError {
    DfmError::InvalidData("encrypted file is truncated".into())
}

fn read_container_header<R: Read>(reader: &mut R) -> Result<ContainerHeader, DecryptError> {
    let mut preamble = [0u8; 12];
    reader.read_exact(&mut preamble).map_err(|_| truncated())?;
    if &preamble[..7] != MAGIC {
        return Err(DecryptError::Invalid(DfmError::InvalidData(
            "not a dfm encrypted file (bad magic)".into(),
        )));
    }
    let version = preamble[7];
    if version != FORMAT_VERSION {
        return Err(DecryptError::Invalid(DfmError::InvalidData(format!(
            "unsupported encrypted format version {}",
            version
        ))));
    }
    let header_len = read_u32_le(&preamble, 8)? as usize;
    if header_len != HEADER_FIXED {
        return Err(DecryptError::Invalid(DfmError::InvalidData(format!(
            "unsupported encrypted header length {}",
            header_len
        ))));
    }
    let mut raw = vec![0u8; HEADER_FIXED];
    reader.read_exact(&mut raw).map_err(|_| truncated())?;

    let m_cost = read_u32_le(&raw, 0)?;
    let t_cost = read_u32_le(&raw, 4)?;
    let p_cost = read_u32_le(&raw, 8)?;
    if m_cost > MAX_KDF_M_COST_KIB || t_cost > MAX_KDF_T_COST || p_cost > MAX_KDF_P_COST {
        return Err(DecryptError::Invalid(DfmError::InvalidData(format!(
            "unreasonable KDF parameters in encrypted header: m_cost={}, t_cost={}, p_cost={}",
            m_cost, t_cost, p_cost
        ))));
    }
    if m_cost < MIN_KDF_M_COST_KIB || t_cost < MIN_KDF_T_COST || p_cost < MIN_KDF_P_COST {
        warn!(
            "encrypted file uses weak KDF parameters (m_cost={} KiB, t_cost={}, p_cost={}, \
             below the {} KiB / t{} / p{} floor); an older dfm build keyed the cost to its \
             build profile and wrote {} KiB / t{} / p{} archives, which a wordlist cracks in \
             minutes. This file is still readable, but re-adding it with a current dfm will \
             rewrite it at full cost",
            m_cost,
            t_cost,
            p_cost,
            MIN_KDF_M_COST_KIB,
            MIN_KDF_T_COST,
            MIN_KDF_P_COST,
            LEGACY_DEBUG_KDF.m_cost_kib,
            LEGACY_DEBUG_KDF.t_cost,
            LEGACY_DEBUG_KDF.p_cost,
        );
    }
    let chunk_size = read_u32_le(&raw, 52)?;
    if !(MIN_CHUNK_SIZE..=MAX_CHUNK_SIZE).contains(&chunk_size) {
        return Err(DecryptError::Invalid(DfmError::InvalidData(format!(
            "unreasonable chunk size in encrypted header: {}",
            chunk_size
        ))));
    }
    Ok(ContainerHeader {
        salt: raw[12..12 + SALT_LEN].try_into().unwrap(),
        base_nonce: raw[12 + SALT_LEN..12 + SALT_LEN + NONCE_LEN]
            .try_into()
            .unwrap(),
        plaintext_len: read_u64_le(&raw, 56)?,
        chunk_size: chunk_size as usize,
        m_cost,
        t_cost,
        p_cost,
        raw,
    })
}

/// Parse the metadata section at the start of the first plaintext chunk.
fn parse_metadata(first_chunk: &[u8]) -> Result<(PlainMeta, usize), DecryptError> {
    let metadata_len = read_u32_le(first_chunk, 0)? as usize;
    if 4 + metadata_len > first_chunk.len() {
        return Err(DecryptError::Invalid(DfmError::InvalidData(
            "encrypted metadata is truncated".into(),
        )));
    }
    let metadata = &first_chunk[4..4 + metadata_len];
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
    Ok((
        PlainMeta {
            file_mode,
            inner_name,
            dirs,
        },
        4 + metadata_len,
    ))
}

/// Streaming decryptor positioned after the first (metadata-carrying) chunk.
/// `stream_rest` pushes the remaining plaintext into any `Write` sink.
struct DecryptSession<R: Read> {
    reader: R,
    cipher: XChaCha20Poly1305,
    header: ContainerHeader,
    /// Plaintext of the first chunk after the metadata section.
    leftover: Vec<u8>,
    next_index: u64,
    /// Plaintext bytes still expected after the first chunk.
    remaining: u64,
}

impl<R: Read> DecryptSession<R> {
    /// Parse the container, derive the key and decrypt+verify the FIRST chunk.
    /// An authentication failure here is reported as [`DecryptError::WrongPassword`]
    /// — with an intact header that is overwhelmingly the likeliest cause, and
    /// it lets callers re-prompt before any output byte is written.
    fn open(mut reader: R, password: &str) -> Result<(Self, PlainMeta), DecryptError> {
        use chacha20poly1305::aead::AeadInPlace;
        use chacha20poly1305::aead::KeyInit;
        use chacha20poly1305::{Tag, XNonce};

        let header = read_container_header(&mut reader)?;
        let key = derive_key(
            password,
            &header.salt,
            header.m_cost,
            header.t_cost,
            header.p_cost,
        )?;
        let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|e| DecryptError::Invalid(DfmError::other(e.to_string())))?;

        let first_pt = std::cmp::min(header.chunk_size as u64, header.plaintext_len) as usize;
        let mut buf = vec![0u8; first_pt + TAG_LEN];
        reader.read_exact(&mut buf).map_err(|_| truncated())?;
        let last0 = header.plaintext_len == first_pt as u64;
        let nonce = stream_nonce(&header.base_nonce, 0, last0);
        let aad = chunk_aad(&header.raw, 0, last0);
        let mut tag_bytes = [0u8; TAG_LEN];
        tag_bytes.copy_from_slice(&buf[first_pt..]);
        cipher
            .decrypt_in_place_detached(
                XNonce::from_slice(&nonce),
                &aad,
                &mut buf[..first_pt],
                Tag::from_slice(&tag_bytes),
            )
            .map_err(|_| DecryptError::WrongPassword)?;

        let (meta, meta_end) = parse_metadata(&buf[..first_pt])?;
        let leftover = buf[meta_end..first_pt].to_vec();

        Ok((
            DecryptSession {
                reader,
                cipher,
                leftover,
                next_index: 1,
                remaining: header.plaintext_len - first_pt as u64,
                header,
            },
            meta,
        ))
    }

    /// Write all remaining plaintext (starting with the first chunk's
    /// non-metadata tail) to `out`, verifying every chunk. A failure past the
    /// first chunk is reported as one authentication failure covering both a
    /// wrong password and a corrupted file: the first chunk has already been
    /// verified by `open`, so either cause is equally likely here, and
    /// distinguishing them is not possible without branching on an error value
    /// that depends on the (secret) key.
    fn stream_rest(mut self, out: &mut dyn Write) -> Result<(), DfmError> {
        use chacha20poly1305::aead::AeadInPlace;
        use chacha20poly1305::{Tag, XNonce};

        out.write_all(&std::mem::take(&mut self.leftover))
            .map_err(|e| io_err(Path::new("<output>"), e))?;

        let chunk_size = self.header.chunk_size;
        let mut buf = vec![0u8; chunk_size + TAG_LEN];
        while self.remaining > 0 {
            let want = std::cmp::min(chunk_size as u64, self.remaining) as usize;
            self.reader
                .read_exact(&mut buf[..want + TAG_LEN])
                .map_err(|_| truncated())?;
            let last = self.remaining == want as u64;
            let nonce = stream_nonce(&self.header.base_nonce, self.next_index, last);
            let aad = chunk_aad(&self.header.raw, self.next_index, last);
            let mut tag_bytes = [0u8; TAG_LEN];
            tag_bytes.copy_from_slice(&buf[want..want + TAG_LEN]);
            self.cipher
                .decrypt_in_place_detached(
                    XNonce::from_slice(&nonce),
                    &aad,
                    &mut buf[..want],
                    Tag::from_slice(&tag_bytes),
                )
                .map_err(|_| {
                    DfmError::InvalidData(
                        "wrong password or the encrypted file is corrupted (authentication failed)"
                            .into(),
                    )
                })?;
            out.write_all(&buf[..want])
                .map_err(|e| io_err(Path::new("<output>"), e))?;
            self.remaining -= want as u64;
            self.next_index += 1;
            if !last && self.next_index >= MAX_STREAM_CHUNKS {
                return Err(DfmError::InvalidData("encrypted stream is too long".into()));
            }
        }

        // The declared total length was authenticated; anything after the
        // final chunk is tampering.
        let mut probe = [0u8; 1];
        if self.reader.read(&mut probe)? > 0 {
            return Err(DfmError::InvalidData(
                "trailing bytes after the final encrypted chunk".into(),
            ));
        }
        Ok(())
    }
}

/// In-memory convenience wrapper over the streaming session (used by tests).
#[cfg(test)]
fn decrypt_bytes(data: &[u8], password: &str) -> Result<Decrypted, DecryptError> {
    let (session, meta) = DecryptSession::open(std::io::Cursor::new(data), password)?;
    let mut content = Vec::new();
    session
        .stream_rest(&mut content)
        .map_err(DecryptError::from)?;
    Ok(Decrypted {
        content,
        file_mode: meta.file_mode,
        inner_name: meta.inner_name,
        dirs: meta.dirs,
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
/// `inner_name`'s parent under the (expanded) target directory, so a decrypt
/// can recreate e.g. a 0700 SSH directory.
fn enclosing_dirs_with_modes(target_dir_abs: &Path, inner_name: &Path) -> Vec<(PathBuf, u32)> {
    enclosing_dirs(inner_name)
        .into_iter()
        .filter_map(|dir_rel| {
            let dir_abs = target_dir_abs.join(&dir_rel);
            fs::metadata(&dir_abs)
                .ok()
                .map(|m| (dir_rel, m.permissions().mode()))
        })
        .collect()
}

// Public writer/reader used by add / pull / merge / purge.

/// Print the "needs an encryption password" info line for the given file's
/// target-relative name, exactly as `add` does before encrypting. Shared with
/// the `diff` command so a password prompt (prompting or decrypting) is always
/// preceded by the same "which file needs a password" info.
pub fn announce_encryption_password(inner_name: &str) {
    eprintln!("file {:?} needs an encryption password", inner_name);
}
/// Encrypt `target_file_path` into a new `*.encrypted` source file at
/// `source_file_path`, recording the file's permissions and the permissions of
/// every enclosing managed directory. The plaintext streams from disk; only
/// one chunk is held in memory. The output is written to a temporary sibling
/// and renamed into place, so an interrupted run leaves no partial source.
pub fn write_encrypted_file(
    settings: &Settings,
    target_file_path: &Path,
    source_file_path: &Path,
) -> Result<(), DfmError> {
    write_encrypted_source(
        settings,
        target_file_path,
        target_file_path,
        source_file_path,
    )
}

/// Encrypt the plaintext of `content_path` to `source_file_path` as a dfm blob.
/// `target_file_path` is used only as the metadata source — the recorded inner
/// name, the file mode and the enclosing-directory modes are the target's, so
/// the blob matches what encrypting the target itself would produce. Needed by
/// `diff --editable`, where the edited plaintext lives in a scratch copy while
/// the target file still carries the authoritative metadata.
pub fn write_encrypted_source(
    settings: &Settings,
    content_path: &Path,
    target_file_path: &Path,
    source_file_path: &Path,
) -> Result<(), DfmError> {
    // Ensure the parent directory exists (important when the source path has
    // subdirectories).
    if let Some(parent) = source_file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }

    let target_metadata =
        fs::metadata(target_file_path).map_err(|e| io_err(target_file_path, e))?;
    let target_file_permissions = target_metadata.permissions();

    let target_dir_path = calc_working_dir_paths_unchecked(settings)?.0;
    let inner_name_p = file_path_relative_to(target_file_path, &target_dir_path);
    let inner_name = inner_name_p.to_string_lossy().into_owned();
    let dirs = enclosing_dirs_with_modes(&target_dir_path, &inner_name_p);

    announce_encryption_password(&inner_name);

    let password = obtain_password(settings)?;
    let prefix = serialize_metadata(&inner_name, target_file_permissions.mode(), &dirs);
    let content_len = fs::metadata(content_path)
        .map_err(|e| io_err(content_path, e))?
        .len();
    encrypt_to_new_file(
        &password,
        || fs::File::open(content_path).map_err(|e| io_err(content_path, e)),
        content_len,
        &prefix,
        source_file_path,
    )
}

/// Stream-encrypt into `dest`, writing through a `.part` sibling and renaming
/// on success so a failure never leaves a truncated output file behind. The
/// temp file is created 0600 with `O_EXCL`: it holds ciphertext whose
/// destination may later be 0600, and a predictable name must not be
/// pre-creatable by another user.
fn encrypt_to_new_file(
    password: &str,
    open_input: impl Fn() -> Result<std::fs::File, DfmError>,
    content_len: u64,
    metadata_prefix: &[u8],
    dest: &Path,
) -> Result<(), DfmError> {
    let part = PathBuf::from(format!("{}.part", dest.display()));
    // Clear a stale temp file from an interrupted run so `create_new` succeeds.
    let _ = fs::remove_file(&part);
    let result = (|| -> Result<(), DfmError> {
        let mut input = open_input()?;
        let temp = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&part)
            .map_err(|e| io_err(&part, e))?;
        let mut out = BufWriter::new(temp);
        encrypt_stream(password, &mut input, content_len, metadata_prefix, &mut out)
    })();
    match result {
        Ok(()) => {
            fs::rename(&part, dest).map_err(|e| io_err(dest, e))?;
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&part);
            Err(e)
        }
    }
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

    // Recorded directory paths are target-relative, so they resolve against
    // the target directory itself.
    let restore_root = calc_working_dir_paths_unchecked(settings)?.0;
    let (session, meta) = open_with_retry(settings, source_file_path, || {
        fs::File::open(source_file_path).map_err(|e| io_err(source_file_path, e))
    })?;
    restore_streamed(&restore_root, target_file_path, &meta, |out| {
        session.stream_rest(out)
    })
}

/// Decrypt an encrypted source file into `dest`, streaming one chunk at a
/// time. Unlike [`read_encrypted_file`] this restores no recorded directory or
/// file permissions: `dest` is a caller-owned path (a diff scratch copy), not
/// the managed target. The destination is created 0600 because it holds
/// plaintext.
pub fn read_encrypted_to_path(
    settings: &Settings,
    source_file_path: &Path,
    dest: &Path,
) -> Result<(), DfmError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let (session, _meta) = open_with_retry(settings, source_file_path, || {
        fs::File::open(source_file_path).map_err(|e| io_err(source_file_path, e))
    })?;
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dest)
        .map_err(|e| io_err(dest, e))?;
    let mut out = BufWriter::new(file);
    session.stream_rest(&mut out)?;
    out.flush().map_err(|e| io_err(dest, e))
}

/// Prompt for the password (retrying once on a wrong password) and open the
/// encrypted file at `encrypted_path`. The first chunk is decrypted and
/// verified before any output file is touched, so a wrong password never
/// leaves partial output behind. `reopen` must yield a fresh reader on every
/// call (the retry restarts from the beginning of the stream).
fn open_with_retry<R: Read, F>(
    settings: &Settings,
    encrypted_path: &Path,
    reopen: F,
) -> Result<(DecryptSession<R>, PlainMeta), DfmError>
where
    F: Fn() -> Result<R, DfmError>,
{
    let mut already_retried = false;
    loop {
        let password = obtain_password(settings)?;
        match DecryptSession::open(reopen()?, &password) {
            Ok(opened) => return Ok(opened),
            Err(DecryptError::WrongPassword) if !already_retried => {
                clear_password_cache();
                eprintln!("wrong password for {:?}, please try again.", encrypted_path);
                already_retried = true;
            }
            Err(DecryptError::WrongPassword) => {
                // Drop the rejected password so the next encrypted file starts
                // with a fresh prompt instead of failing on the cached wrong one.
                clear_password_cache();
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

/// Write the decrypted content to the target while it streams through `sink`,
/// restoring directory and file permissions that were recorded at encrypt
/// time. A streaming failure removes the partially written target file.
///
/// Nothing outside the target file itself is created or chmod'ed until `sink`
/// has returned `Ok`, i.e. until every chunk has been authenticated: a blob
/// that fails verification must not leave directories behind.
///
/// Directory permissions are restored relative to the caller-supplied restore
/// root, because the recorded directory paths are target-relative (a file
/// recorded as `private/sub/f.conf` records `private` and `private/sub`). For
/// the internal add/pull/merge/purge path this is the expanded target
/// directory; standalone `dfm decrypt` passes the output file's parent, whose
/// blobs record no directories at all.
fn restore_streamed(
    restore_root: &Path,
    target_file_path: &Path,
    meta: &PlainMeta,
    sink: impl FnOnce(&mut dyn Write) -> Result<(), DfmError>,
) -> Result<(), DfmError> {
    if let Some(parent) = target_file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    // Created 0600, not the umask default: the recorded mode is only applied
    // once the content has streamed out and verified, so a file recorded as
    // 0644 would otherwise be world-readable plaintext for the whole decrypt.
    // The later `set_permissions` still sets the recorded mode exactly, so the
    // end state is unchanged.
    let target = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(target_file_path)
        .map_err(|e| io_err(target_file_path, e))?;
    let mut out = BufWriter::new(target);
    let restore_permissions = |mut out: BufWriter<std::fs::File>| -> Result<(), DfmError> {
        out.flush().map_err(|e| io_err(target_file_path, e))?;
        drop(out);
        let restored = apply_dir_modes(restore_root, target_file_path, &meta.dirs).and_then(|()| {
            fs::set_permissions(target_file_path, fs::Permissions::from_mode(meta.file_mode))
                .map_err(|e| io_err(target_file_path, e))
        });
        if let Err(e) = restored {
            let _ = fs::remove_file(target_file_path);
            return Err(e);
        }
        Ok(())
    };

    match sink(&mut out) {
        Ok(()) => restore_permissions(out),
        Err(e) => {
            let _ = fs::remove_file(target_file_path);
            Err(e)
        }
    }
}

/// Recreate enclosing directories with their recorded permissions (e.g. a
/// 0700 SSH directory that `create_dir_all` would otherwise reset).
fn apply_dir_modes(
    target_root: &Path,
    target_file_path: &Path,
    dirs: &[(PathBuf, u32)],
) -> Result<(), DfmError> {
    for (dir_rel, mode) in dirs {
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
    Ok(())
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

// Standalone `dfm encrypt` / `dfm decrypt` commands.

/// Encrypt `input_path` into `output_path` (a standalone file, not bound to a
/// target directory). The recorded inner name is the plain filename and no
/// directory-permission metadata is emitted.
pub fn encrypt_file_standalone(
    settings: &Settings,
    input_path: &Path,
    output_path: &Path,
) -> Result<(), DfmError> {
    let metadata = fs::metadata(input_path).map_err(|e| io_err(input_path, e))?;
    let file_mode = metadata.permissions().mode();
    let inner_name = input_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| input_path.to_string_lossy().into_owned());

    eprintln!("file {:?} needs an encryption password", input_path);
    let password = obtain_password(settings)?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }
    let prefix = serialize_metadata(&inner_name, file_mode, &[]);
    encrypt_to_new_file(
        &password,
        || fs::File::open(input_path).map_err(|e| io_err(input_path, e)),
        metadata.len(),
        &prefix,
        output_path,
    )
}

/// Decrypt an encrypted standalone file `input_path` into `output_path`.
/// Directory-permission metadata (if any) is applied relative to the output
/// file's parent directory.
pub fn decrypt_file_standalone(
    settings: &Settings,
    input_path: &Path,
    output_path: &Path,
) -> Result<(), DfmError> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
    }

    let target_root = output_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    let (session, meta) = open_with_retry(settings, input_path, || {
        fs::File::open(input_path).map_err(|e| io_err(input_path, e))
    })?;
    restore_streamed(&target_root, output_path, &meta, |out| {
        session.stream_rest(out)
    })
}

/// Decrypt an encrypted standalone file `input_path` and stream the plaintext
/// to stdout. The first chunk is verified before anything is written, so a
/// wrong password never emits partial bytes. No directory/file permissions are
/// restored (there is no output file).
pub fn decrypt_to_stdout(settings: &Settings, input_path: &Path) -> Result<(), DfmError> {
    let (session, _meta) = open_with_retry(settings, input_path, || {
        fs::File::open(input_path).map_err(|e| io_err(input_path, e))
    })?;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    session.stream_rest(&mut out)?;
    out.flush().map_err(|e| io_err(Path::new("<stdout>"), e))
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
        let dirs = vec![
            (PathBuf::from("private"), 0o700),
            (PathBuf::from("private/sub"), 0o710),
        ];

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

    // streaming (chunked) format tests

    /// Deterministic pseudo-random content of `len` bytes.
    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn multi_chunk_roundtrip() {
        let content = pattern(3 * CHUNK_SIZE as usize + 12345);
        let blob = encrypt_bytes("pw", &content, 0o600, "big/file.bin", &[]).unwrap();
        let dec = decrypt_bytes(&blob, "pw").unwrap();
        assert_eq!(dec.content, content);
    }

    #[test]
    fn exact_chunk_multiple_roundtrip() {
        for len in [
            1,
            CHUNK_SIZE as usize - 1,
            CHUNK_SIZE as usize,
            CHUNK_SIZE as usize + 1,
            2 * CHUNK_SIZE as usize,
        ] {
            let content = pattern(len);
            let blob = encrypt_bytes("pw", &content, 0o644, "f", &[]).unwrap();
            assert_eq!(decrypt_bytes(&blob, "pw").unwrap().content, content);
        }
    }

    #[test]
    fn truncated_stream_rejected() {
        let content = pattern(2 * CHUNK_SIZE as usize + 100);
        let mut blob = encrypt_bytes("pw", &content, 0o644, "f", &[]).unwrap();
        blob.truncate(blob.len() - 10);
        assert!(matches!(
            decrypt_bytes(&blob, "pw"),
            Err(DecryptError::Invalid(_))
        ));
    }

    #[test]
    fn trailing_junk_rejected() {
        let mut blob = encrypt_bytes("pw", b"data", 0o644, "f", &[]).unwrap();
        blob.extend_from_slice(b"xyz");
        assert!(matches!(
            decrypt_bytes(&blob, "pw"),
            Err(DecryptError::Invalid(_))
        ));
    }

    #[test]
    fn swapped_chunks_rejected() {
        // Three full chunks plus a short final one: swapping two MIDDLE
        // records must fail authentication in stream_rest (a swap involving
        // chunk 0 surfaces as WrongPassword instead — see open()).
        let content = pattern(3 * CHUNK_SIZE as usize + 100);
        let mut blob = encrypt_bytes("pw", &content, 0o644, "f", &[]).unwrap();

        // Records start after magic+version+header_len+header; every
        // non-final record is chunk_size ciphertext + 16-byte tag.
        let body = 12 + HEADER_FIXED;
        let rec = CHUNK_SIZE as usize + TAG_LEN;
        let (a, b) = blob.split_at_mut(body + 2 * rec);
        a[body + rec..body + 2 * rec].swap_with_slice(&mut b[..rec]);

        assert!(matches!(
            decrypt_bytes(&blob, "pw"),
            Err(DecryptError::Invalid(_))
        ));
    }

    #[test]
    fn absurd_chunk_size_rejected() {
        let blob = encrypt_bytes("pw", b"data", 0o644, "a", &[]).unwrap();
        // chunk_size lives at header offset 52 → absolute offset 12 + 52.
        let mut tampered = blob.clone();
        tampered[64..68].copy_from_slice(&(MAX_CHUNK_SIZE + 1).to_le_bytes());
        assert!(matches!(
            decrypt_bytes(&tampered, "pw"),
            Err(DecryptError::Invalid(_))
        ));

        let mut tiny = blob.clone();
        tiny[64..68].copy_from_slice(&(MIN_CHUNK_SIZE - 1).to_le_bytes());
        assert!(matches!(
            decrypt_bytes(&tiny, "pw"),
            Err(DecryptError::Invalid(_))
        ));
    }

    #[test]
    fn kdf_params_are_the_same_for_every_build_profile() {
        // The parameters are written into the header, so a value selected by
        // the build profile would make an archive permanently guessable and
        // would mark it as weak. Both profiles must derive the release cost,
        // and it must satisfy the lower bound enforced when reading.
        const {
            assert!(KDF_M_COST_KIB == 65536);
            assert!(KDF_T_COST == 3);
            assert!(KDF_P_COST == 4);
            assert!(KDF_M_COST_KIB >= MIN_KDF_M_COST_KIB);
            assert!(KDF_T_COST >= MIN_KDF_T_COST);
            assert!(KDF_P_COST >= MIN_KDF_P_COST);
        }
    }

    #[test]
    fn the_legacy_debug_cost_trips_the_warning_floor() {
        // The floor exists to surface archives written by the old
        // profile-dependent builds. If the floor sat at or below that cost, the
        // warning would never fire for the very blobs it was added for.
        const {
            assert!(LEGACY_DEBUG_KDF.m_cost_kib < MIN_KDF_M_COST_KIB);
            assert!(LEGACY_DEBUG_KDF.t_cost < MIN_KDF_T_COST);
            assert!(LEGACY_DEBUG_KDF.p_cost >= MIN_KDF_P_COST);
        }
    }

    /// A container written with the reduced cost an older build would have
    /// used. Built through the real encrypt path so the header is
    /// authenticated and the blob genuinely decrypts.
    fn legacy_weak_blob(password: &str, content: &[u8]) -> Vec<u8> {
        let mut salt = [0u8; SALT_LEN];
        let mut base_nonce = [0u8; NONCE_LEN];
        for (i, b) in salt.iter_mut().enumerate() {
            *b = i as u8;
        }
        for (i, b) in base_nonce.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(7);
        }
        let mut blob = Vec::new();
        encrypt_stream_seeded(
            password,
            &mut std::io::Cursor::new(content),
            content.len() as u64,
            &serialize_metadata("a", 0o644, &[]),
            &mut blob,
            LEGACY_DEBUG_KDF,
            &salt,
            &base_nonce,
        )
        .unwrap();
        blob
    }

    #[test]
    fn weak_kdf_params_stay_readable() {
        // A blob written with reduced cost must still decrypt for its owner;
        // refusing would lock them out of their own files. The weakness is
        // reported, not enforced.
        let blob = legacy_weak_blob("pw", b"data");
        let dec = decrypt_bytes(&blob, "pw").expect("weak blob stays readable");
        assert_eq!(dec.content, b"data");
    }

    #[test]
    fn weak_kdf_params_do_not_weaken_authentication() {
        // The cost travels in the header, which is the AAD of every chunk, so
        // the reduced cost is bound to the ciphertext: editing it to claim a
        // stronger cost must fail rather than silently "upgrade" the blob.
        let blob = legacy_weak_blob("pw", b"data");
        let mut edited = blob.clone();
        let strong = KDF_M_COST_KIB.to_le_bytes();
        edited[12..16].copy_from_slice(&strong);
        assert!(matches!(
            decrypt_bytes(&edited, "pw"),
            Err(DecryptError::WrongPassword)
        ));
    }

    #[test]
    fn empty_password_is_refused() {
        // An empty password derives a valid key, so it would produce an
        // archive that looks protected but is not. A provider that prints
        // nothing, or only whitespace, has failed rather than supplied one.
        for command in ["true", "printf '   '", "printf '\\n'"] {
            let mut settings = crate::create_default_settings();
            settings.obtain_password_shell_command = Some(command.into());
            assert!(
                matches!(obtain_password(&settings), Err(DfmError::InvalidInput(_))),
                "command {command:?} must not yield a usable password"
            );
        }
    }

    #[test]
    fn surrounding_spaces_are_part_of_the_password() {
        // The whitespace check must not silently trim the stored password:
        // " pw " is a different password from "pw".
        let mut settings = crate::create_default_settings();
        settings.obtain_password_shell_command = Some("printf ' pw '".into());
        assert_eq!(&*obtain_password(&settings).unwrap(), " pw ");
        clear_password_cache();
    }

    #[test]
    fn restored_plaintext_is_private_until_its_recorded_mode_is_applied() {
        // The target is created 0600 and only widened to the recorded mode
        // after the content has streamed out and verified, so a file recorded
        // 0644 is never world-readable plaintext mid-decrypt.
        let root = std::env::temp_dir().join(format!("dfm-mode-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let meta = PlainMeta {
            inner_name: "f.conf".to_owned(),
            file_mode: 0o644,
            dirs: vec![],
        };
        let target = root.join("f.conf");
        let mut mode_during = None;

        restore_streamed(&root, &target, &meta, |_out| {
            mode_during = Some(fs::metadata(&target).unwrap().permissions().mode() & 0o777);
            Ok(())
        })
        .unwrap();

        assert_eq!(
            mode_during,
            Some(0o600),
            "plaintext must not be exposed while streaming"
        );
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o644,
            "the recorded mode must still be the end state"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn part_file_is_not_world_readable() {
        // The temp file holds ciphertext whose destination may later be 0600,
        // so it must not be created with the default 0666 & !umask.
        let dir = std::env::temp_dir().join(format!("dfm-part-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let plain = dir.join("plain");
        fs::write(&plain, b"content").unwrap();
        // `dest` is a directory, so the closing rename fails and the temp file
        // survives for inspection. It is only cleared on the next run, by
        // design: the ciphertext it holds is not otherwise recoverable.
        let dest = dir.join("out.encrypted");
        fs::create_dir(&dest).unwrap();

        let result = encrypt_to_new_file(
            "pw",
            || Ok(fs::File::open(&plain).unwrap()),
            7,
            &serialize_metadata("a", 0o644, &[]),
            &dest,
        );
        assert!(result.is_err(), "renaming onto a directory must fail");

        let part = dir.join("out.encrypted.part");
        let mode = fs::metadata(&part).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "part file must be 0600, got {mode:o}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recorded_directory_modes_are_applied_only_after_the_stream_succeeds() {
        // The recorded directory paths and modes arrive in the first chunk, so
        // they are authenticated before `restore_streamed` runs — but the
        // content they guard is not. Applying their modes before the content
        // verifies would let a blob whose later chunks fail authentication
        // still reshape directories in the target tree.
        let root = std::env::temp_dir().join(format!("dfm-restore-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let meta = PlainMeta {
            inner_name: "private/sub/f.conf".to_owned(),
            file_mode: 0o600,
            dirs: vec![
                (PathBuf::from("private"), 0o700),
                (PathBuf::from("private/sub"), 0o710),
            ],
        };
        let target = root.join("private/sub/f.conf");
        let mode_of = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;

        let failed = restore_streamed(&root, &target, &meta, |_out| {
            Err(DfmError::InvalidData("authentication failed".into()))
        });
        assert!(failed.is_err());
        assert!(!target.exists(), "the partial target must be removed");
        if root.join("private").exists() {
            assert_ne!(
                mode_of(&root.join("private")),
                0o700,
                "a recorded directory mode must not be applied to unverified metadata"
            );
        }

        // The same blob, restored successfully, does get its recorded modes.
        restore_streamed(&root, &target, &meta, |out| {
            out.write_all(b"content").map_err(DfmError::Io)
        })
        .unwrap();
        assert_eq!(mode_of(&root.join("private")), 0o700);
        assert_eq!(mode_of(&root.join("private/sub")), 0o710);
        assert_eq!(mode_of(&target), 0o600);
        let _ = fs::remove_dir_all(&root);
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
