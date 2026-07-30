pub(crate) mod add;
pub(crate) mod config;
pub(crate) mod forget;
pub(crate) mod ignore;
pub(crate) mod init;
pub(crate) mod merge;
pub(crate) mod paths;
pub(crate) mod pull;
pub(crate) mod purge;
pub(crate) mod status;

pub(crate) use add::add_command;
pub(crate) use config::config_command;
pub(crate) use forget::forget_command;
pub(crate) use ignore::ignore_command;
pub(crate) use init::init_command;
pub(crate) use merge::merge_command;
pub(crate) use paths::paths_command;
pub(crate) use pull::pull_command;
pub(crate) use purge::purge_command;
pub(crate) use status::status_command;

use std::fs;
use crate::DfmError;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::SystemTime;
use filetime_creation::{set_file_mtime, FileTime};
use log::{error, info, trace, log_enabled};
use regex::RegexSet;

use dfm::*;

// ---------------------------------------------------------------------------
// Shared state helpers
// ---------------------------------------------------------------------------

/// Get the sync time for a file, computing the state key from its absolute path.
pub(crate) fn get_sync_time<'a>(
    state: &'a StateObject,
    path: &PathBuf,
    source_dir: &PathBuf,
) -> Option<&'a SyncTime> {
    let rel = file_path_relative_to(path, source_dir);
    let rel = remove_dots_from_path(&rel);
    state.syncs.get(rel.to_str().unwrap())
}

/// Remove the sync entry for a file, computing the state key from its absolute path.
pub(crate) fn remove_sync_state(
    state: &mut StateObject,
    path: &PathBuf,
    source_dir: &PathBuf,
) {
    let rel = file_path_relative_to(path, source_dir);
    let rel = remove_dots_from_path(&rel);
    state.syncs.remove(rel.to_str().unwrap());
}

/// Insert/update a sync entry and set mtimes on both the source-side file
/// and the target-side file.
pub(crate) fn update_sync_state(
    state: &mut StateObject,
    source_abs: &PathBuf,
    target_abs: &PathBuf,
    source_dir_abs: &PathBuf,
) -> Result<(), DfmError> {
    let sync_creation = SystemTime::now();
    let source_rel_path = file_path_relative_to(source_abs, source_dir_abs);
    let source_rel_path = remove_dots_from_path(&source_rel_path);
    state.syncs.insert(source_rel_path.to_str().unwrap().to_string(), SyncTime(sync_creation));
    let ft = FileTime::from_system_time(sync_creation);
    set_file_mtime(target_abs, ft)?;
    set_file_mtime(source_abs, ft)?;
    Ok(())
}

/// Convert a source-relative path (state key) to a target-relative path,
/// stripping encrypted/symlink postfixes.
pub(crate) fn source_rel_to_target_rel(
    source_rel: &str,
    dot_prefix: &str,
    symlink_postfix: &str,
    encrypted_postfix: &str,
) -> String {
    let mut target_rel = source_rel.replace(dot_prefix, ".");
    if target_rel.ends_with(symlink_postfix) {
        target_rel = target_rel[..target_rel.len() - symlink_postfix.len()].to_string();
    } else if target_rel.ends_with(encrypted_postfix) {
        target_rel = target_rel[..target_rel.len() - encrypted_postfix.len()].to_string();
    }
    target_rel
}

/// Thin wrapper around `list_directory` that bundles the error check.
pub(crate) fn list_directory_or_error(
    paths: &[PathBuf],
    filter: Option<&RegexSet>,
    context: &str,
) -> Result<Vec<PathBuf>, DfmError> {
    let ListDirectories { found, errors, .. } = list_directory(paths, filter)?;
    if !errors.is_empty() {
        return Err(DfmError::InvalidData(
            format!("failed to process some subdirectories or files {}: {:?}", context, errors)
        ));
    }
    Ok(found)
}

// ---------------------------------------------------------------------------
// Shared message templates
// ---------------------------------------------------------------------------

/// Message printed when --dry-run is active.
pub(crate) fn msg_dry_run() -> &'static str {
    "dry run specified, no changes will be made"
}

/// Message printed when there is nothing to do.
pub(crate) fn msg_nothing_to_do() -> &'static str {
    "nothing to do"
}

// ---------------------------------------------------------------------------
// Shared --dry-run / --force helpers
// ---------------------------------------------------------------------------

/// Resolve the effective dry-run value: `true` if either the per-command flag
/// *or* the global `--dry-run` flag is set.
#[inline]
pub(crate) fn resolve_dry_run(cmd_dry_run: bool, args_dry_run: bool) -> bool {
    cmd_dry_run || args_dry_run
}

/// If `force` is `false`, return `Err(DfmError::Other(msg))`.
/// Useful for the common post-loop "force required" check.
///
/// When `force` is `true` the caller still needs to handle the case
/// (e.g. skip the conflict, or proceed despite errors); this helper
/// only covers the "reject without force" half.
#[inline]
pub(crate) fn require_force(force: bool, msg: impl std::fmt::Display) -> Result<(), DfmError> {
    if force {
        Ok(())
    } else {
        Err(DfmError::Other(msg.to_string()))
    }
}

/// Shared copy + permissions + mtime + state update logic used by both
/// `add` (target → source) and `pull` (source → target).
///
/// `from` is the source of the copy (its permissions are preserved).
/// `to`   is the destination.
/// `source_file_in_source_dir` — the file residing in the source directory,
/// used to compute the state key.
pub(crate) fn sync_file_copy(
    from: &PathBuf,
    to: &PathBuf,
    source_file_in_source_dir: &PathBuf,
    state: &mut StateObject,
    source_dir_abs_path: &PathBuf,
) -> Result<(), DfmError> {
    fs::create_dir_all(to.parent().unwrap())?;
    fs::copy(from, to)?;

    let permissions = from.metadata()?.permissions();
    trace!("copy permissions {:o}", permissions.mode());
    if let Err(e) = fs::set_permissions(to.clone(), permissions.clone()) {
        error!("failed to set permissions {:?} to {:?}: {}", permissions.mode(), to, e);
    }

    // `source_file_in_source_dir` is always the source-dir file.  The
    // "other" file (whichever of `from`/`to` is not the source-dir file)
    // must be passed as `target_abs` so `update_sync_state` sets its mtime.
    // In pull: from == source_file_in_source_dir, to = the target file.
    // In add:  to   == source_file_in_source_dir, from = the target file.
    let other = if source_file_in_source_dir == from { to } else { from };
    update_sync_state(state, source_file_in_source_dir, other, source_dir_abs_path)?;

    if log_enabled!(log::Level::Trace) {
        let from_meta = from.metadata()?;
        let to_meta = to.metadata()?;
        let to_modified = to_meta.modified()?;
        let from_modified = from_meta.modified()?;
        trace!("final state:\n from: mtime={:?}\n to: mtime={:?}",
             to_modified, from_modified);
    }

    Ok(())
}

/// Resolve the merge command from settings.
fn resolve_merge_command(settings: &Settings) -> Result<String, DfmError> {
    if let Some(ref cmd) = settings.merge_tool_command {
        if !cmd.is_empty() {
            return Ok(cmd.clone());
        }
    }
    Err(DfmError::Other(
        "no merge tool configured — set merge_tool_command in config".into()
    ))
}

/// Run the merge tool inside `.current_merge/` in the source directory.
///
/// Creates temporary copies named `target.<file>` (working dir side),
/// `source.<file>` (cellar side, decrypted if encrypted) and an empty
/// `result.<file>` for the merge tool's output.  The merge tool must
/// write the merged result into `{result}` — after it succeeds the
/// result file is copied back to both the target and the source, and
/// the sync state is updated.  The `.current_merge/` directory is
/// removed before returning.
pub(crate) fn run_merge(
    settings: &Settings,
    source_abs_path: &PathBuf,
    target_abs_path: &PathBuf,
    state: &mut StateObject,
    source_dir_abs_path: &PathBuf,
) -> Result<(), DfmError> {
    let source_dir = PathBuf::from(&settings.source_dir);
    let merge_dir = source_dir.join(".current_merge");
    fs::create_dir_all(&merge_dir)?;

    let file_name = target_abs_path
        .file_name()
        .ok_or_else(|| DfmError::Other("target path has no file name".into()))?
        .to_str()
        .ok_or_else(|| DfmError::Other("target file name is not valid UTF-8".into()))?;

    let source_is_encrypted = source_abs_path
        .to_str()
        .map(|s| s.ends_with(&settings.encrypted_postfix))
        .unwrap_or(false);

    // Copy both sides into the merge directory:
    //   target.<file> = working-dir side (always plain text)
    //   source.<file> = cellar side (decrypted if encrypted)
    //   result.<file> = merge tool writes output here
    let target_path = merge_dir.join(format!("target.{}", file_name));
    let source_path = merge_dir.join(format!("source.{}", file_name));
    let result_path = merge_dir.join(format!("result.{}", file_name));
    fs::copy(target_abs_path, &target_path)?;
    if source_is_encrypted {
        dfm::crypt::read_zip_file(settings, source_abs_path, &source_path)?;
    } else {
        fs::copy(source_abs_path, &source_path)?;
    }
    let command = resolve_merge_command(settings)?;

    // Parse command template: first token is the program, rest are arguments
    // with {target}, {source} and {result} replaced by actual temp file paths.
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        let _ = fs::remove_dir_all(&merge_dir);
        return Err(DfmError::Other("merge command is empty".into()));
    }
    let (prog, args) = parts.split_first().unwrap();
    let args: Vec<String> = args.iter().map(|a| {
        a.replace("{target}", target_path.to_str().unwrap())
         .replace("{source}", source_path.to_str().unwrap())
         .replace("{result}", result_path.to_str().unwrap())
    }).collect();

    info!("running merge tool: {} {:?}", prog, args);

    let status = std::process::Command::new(prog)
        .args(&args)
        .status()
        .map_err(|e| {
            let _ = fs::remove_dir_all(&merge_dir);
            DfmError::Io(e)
        })?;

    if !status.success() || !result_path.exists() {
        let _ = fs::remove_dir_all(&merge_dir);
        let reason = if !status.success() {
            format!("merge tool exited with status {}", status)
        } else {
            format!("merge tool exited successfully but did not create {:?}", result_path)
        };
        return Err(DfmError::Other(reason));
    }

    // Copy the merged result to BOTH the source and the target
    if source_is_encrypted {
        dfm::crypt::write_zip_file(settings, &result_path, source_abs_path)?;
    } else {
        fs::copy(&result_path, source_abs_path)?;
    }
    fs::copy(&result_path, target_abs_path)?;

    // Update sync state and mtimes
    update_sync_state(state, source_abs_path, target_abs_path, source_dir_abs_path)?;

    let _ = fs::remove_dir_all(&merge_dir);

    info!("merge completed for {:?}", target_abs_path);
    Ok(())
}
