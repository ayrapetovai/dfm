pub(crate) mod add;
pub(crate) mod config;
pub(crate) mod forget;
pub(crate) mod ignore;
pub(crate) mod init;
pub(crate) mod paths;
pub(crate) mod pull;
pub(crate) mod purge;
#[cfg(test)]
pub(crate) mod tests;

pub(crate) use add::add_command;
pub(crate) use config::config_command;
pub(crate) use forget::forget_command;
pub(crate) use ignore::ignore_command;
pub(crate) use init::init_command;
pub(crate) use paths::paths_command;
pub(crate) use pull::pull_command;
pub(crate) use purge::purge_command;

use std::fs;
use crate::DfmError;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::SystemTime;
use filetime_creation::{set_file_mtime, FileTime};
use log::{error, trace, log_enabled};

use dfm::*;

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

    let sync_creation = SystemTime::now();
    let source_rel_path = file_path_relative_to(source_file_in_source_dir, source_dir_abs_path);
    let source_rel_path = remove_dots_from_path(&source_rel_path);
    state.syncs.insert(source_rel_path.to_str().unwrap().to_string(), sync_creation);

    let sync_creation = FileTime::from_system_time(sync_creation);

    set_file_mtime(to, sync_creation)?;
    set_file_mtime(from, sync_creation)?;

    if log_enabled!(log::Level::Trace) {
        let from_meta = from.metadata()?;
        let to_meta = to.metadata()?;

        let to_modified = to_meta.modified()?;
        let from_modified = from_meta.modified()?;

        trace!("final state:\n from: mtime={:?}\n to: sync={:?},\n      mtime={:?}",
             to_modified, sync_creation, from_modified);
    }

    Ok(())
}
