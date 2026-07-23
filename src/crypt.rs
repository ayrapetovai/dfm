use std::process::{Command, Stdio};
use std::fs;
use std::io::Write;
use crate::DfmError;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use log::debug;
use zip::write::SimpleFileOptions;
// #[cfg(all(feature = "aes-crypto", feature = "zstd"))]
use zip::{AesMode, CompressionMethod::Bzip2};

use crate::{Settings, file_path_relative_to};

pub fn write_zip_file(settings: &Settings, target_file_path: &PathBuf, source_file_path: &PathBuf) -> Result<(), DfmError> {
    // Ensure the parent directory exists (important when source path has subdirectories)
    if let Some(parent) = source_file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(source_file_path.as_path())?;
    let mut zip = zip::ZipWriter::new(file);

    let target_file_permissions = fs::metadata(target_file_path)?.permissions();

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

fn default_read_password() -> Result<String, DfmError> {
    let config = rpassword::ConfigBuilder::new()
         .output_discard()
         .password_feedback_mask('*')
         .build();

    rpassword::read_password_with_config(config)
        .map_err(|e| DfmError::other(format!("failed to read password: {}", e)))
}
