use std::path::PathBuf;

use dfm::*;
use crate::DfmError;

/// Typed arguments for the standalone `encrypt` command.
pub struct EncryptArgs {
    pub path: PathBuf,
    pub output: Option<PathBuf>,
}

/// Typed arguments for the standalone `decrypt` command.
pub struct DecryptArgs {
    pub path: PathBuf,
    pub output: Option<PathBuf>,
}

/// Encrypt a single file with a password into a self-contained dfm blob.
/// Without `--output`, writes `<path>.encrypted` next to the input.
pub fn encrypt_command(settings: &Settings, args: EncryptArgs) -> Result<(), DfmError> {
    let output = match &args.output {
        Some(o) => o.clone(),
        None => {
            let mut name = args.path.as_os_str().to_owned();
            name.push(settings.encrypted_postfix.as_str());
            PathBuf::from(name)
        }
    };
    dfm::crypt::encrypt_file_standalone(settings, &args.path, &output)?;
    println!("{} -> {}", args.path.display(), output.display());
    Ok(())
}

/// Decrypt a dfm-encrypted file. Without `--output`, strips the encrypted
/// postfix from the input name (`.encrypted` → plain) in the current
/// directory; if the name has no postfix, an explicit `--output` is required.
pub fn decrypt_command(settings: &Settings, args: DecryptArgs) -> Result<(), DfmError> {
    let output = match &args.output {
        Some(o) => o.clone(),
        None => {
            let name = args.path.to_string_lossy();
            let stripped = name.strip_suffix(&settings.encrypted_postfix);
            match stripped {
                Some(s) => PathBuf::from(s),
                None => {
                    return Err(DfmError::InvalidInput(format!(
                        "input {:?} has no {} suffix; pass --output to choose the output path",
                        args.path, settings.encrypted_postfix
                    )));
                }
            }
        }
    };
    dfm::crypt::decrypt_file_standalone(settings, &args.path, &output)?;
    println!("{} -> {}", args.path.display(), output.display());
    Ok(())
}
