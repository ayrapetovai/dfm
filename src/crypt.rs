use std::process::{Command, Stdio};
use std::fs;
use std::io::Write;
use crate::DfmError;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Mutex;
use log::debug;
use zip::write::SimpleFileOptions;
// #[cfg(all(feature = "aes-crypto", feature = "zstd"))]
use zip::{AesMode, CompressionMethod::Bzip2};

use crate::{Settings, file_path_relative_to};

// ---------------------------------------------------------------------------
// Password cache — ask only once per `dfm` process
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

    let shell_env = "SHELL";
    let shell_env_value = if envmnt::exists(shell_env) {
        Some(envmnt::get_any(&vec![shell_env], ""))
    } else {
        None
    };

    debug!("get password command is set to {:?}", settings.obtain_password_shell_command);
    debug!("shell {:?}", shell_env_value);

    let password = if let Some(get_password_command) = settings.obtain_password_shell_command.clone() &&
            !get_password_command.is_empty() &&
            let Some(shell) = shell_env_value {
        debug!("launching get password program");

        // FIXME looks very unsecure
        let child = Command::new(shell)
            .args(["-c", get_password_command.as_str()])
            .stdin(Stdio::inherit())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DfmError::other(format!("Error (return code {}): {}", output.status.code().unwrap_or(-1), stderr)));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout.to_string()
    } else {
        debug!("using default procedure to get password");
        default_read_password()?
    };

    // Cache for subsequent calls
    set_cached_password(password.clone());
    Ok(password)
}

pub fn write_zip_file(settings: &Settings, target_file_path: &PathBuf, source_file_path: &PathBuf) -> Result<(), DfmError> {
    // Ensure the parent directory exists (important when source path has subdirectories)
    if let Some(parent) = source_file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(source_file_path.as_path())?;
    let mut zip = zip::ZipWriter::new(file);

    let target_file_permissions = fs::metadata(target_file_path)?.permissions();

    let password = obtain_password(settings)?;

    let target_dir_path = PathBuf::from(&settings.target_dir);
    let inner_name = file_path_relative_to(target_file_path, &target_dir_path);

    let inner_name_str = inner_name.to_str()
        .ok_or_else(|| DfmError::InvalidData("non-UTF-8 path in zip entry".into()))?;
    zip.start_file(
        inner_name_str,
        SimpleFileOptions::default()
            .compression_method(Bzip2)
            .with_aes_encryption(AesMode::Aes256, &password)
            .unix_permissions(target_file_permissions.mode()),
    ).map_err(DfmError::other)?;

    let file_content = fs::read_to_string(target_file_path)?;
    zip.write_all(file_content.as_bytes()).map_err(DfmError::other)?;
    zip.finish().map_err(DfmError::other)?;

    Ok(())
}

/// Decrypt a zip archive created by `write_zip_file` and write its content
/// to the given target path.  The archive must contain exactly one entry.
///
/// On `InvalidPassword` the cache is cleared and the user is reprompted once.
/// If the second attempt also fails the error is returned.
pub fn read_zip_file(settings: &Settings, source_zip_path: &PathBuf, target_file_path: &PathBuf) -> Result<(), DfmError> {
    // Ensure the target parent directory exists
    if let Some(parent) = target_file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut already_retried = false;

    loop {
        let password = obtain_password(settings)?;

        // Open a fresh archive on each attempt: a failed by_index_decrypt
        // advances the internal reader position, invalidating the archive.
        let file = std::fs::File::open(source_zip_path)?;
        let mut archive = zip::ZipArchive::new(file).map_err(DfmError::other)?;

        match archive.by_index_decrypt(0, password.as_bytes()) {
            Ok(mut zip_file) => {
                let mut output_file = std::fs::File::create(target_file_path)?;
                std::io::copy(&mut zip_file, &mut output_file).map_err(DfmError::other)?;
                return Ok(());
            }
            Err(zip::result::ZipError::InvalidPassword) if !already_retried => {
                clear_password_cache();
                eprintln!("Wrong password for {:?}, please try again.", source_zip_path);
                already_retried = true;
                // Loop back — obtain_password will re-prompt since the cache was cleared
            }
            Err(e) => return Err(DfmError::other(e)),
        }
    }
}

fn default_read_password() -> Result<String, DfmError> {
    let config = rpassword::ConfigBuilder::new()
         .output_discard()
         .password_feedback_mask('*')
         .build();

    rpassword::read_password_with_config(config)
        .map_err(|e| DfmError::other(format!("failed to read password: {}", e)))
}
