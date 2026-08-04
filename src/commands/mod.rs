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

pub(crate) use add::AddArgs;
pub(crate) use config::ConfigArgs;
pub(crate) use forget::ForgetArgs;
pub(crate) use ignore::IgnoreArgs;
pub(crate) use init::InitArgs;
pub(crate) use merge::MergeArgs;
pub(crate) use paths::PathsArgs;
pub(crate) use pull::PullArgs;
pub(crate) use purge::PurgeArgs;
pub(crate) use status::StatusArgs;

use std::fs;
use crate::DfmError;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use filetime_creation::{set_file_mtime, FileTime};
use microxdg::Xdg;
use log::{error, info, trace, log_enabled};

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
    let rel_str = rel.to_string_lossy();
    state.syncs.get(rel_str.as_ref())
}

/// Remove the sync entry for a file, computing the state key from its absolute path.
pub(crate) fn remove_sync_state(
    state: &mut StateObject,
    path: &PathBuf,
    source_dir: &PathBuf,
) {
    let rel = file_path_relative_to(path, source_dir);
    let rel = remove_dots_from_path(&rel);
    let rel_str = rel.to_string_lossy();
    state.syncs.remove(rel_str.as_ref());
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
    let sha256 = compute_sha256(source_abs)?;
    state.syncs.insert(source_rel_path.to_string_lossy().into_owned(), SyncTime { mtime: sync_creation, sha256 });
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
    let mut target_rel = decode_source_rel_path(source_rel, dot_prefix, true)
        .to_string_lossy()
        .into_owned();
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
    rel_base: &PathBuf,
    filter: Option<TraversalFilter<'_>>,
    context: &str,
) -> Result<Vec<PathBuf>, DfmError> {
    let ListDirectories { found, errors, .. } = list_directory(paths, rel_base, filter)?;
    if !errors.is_empty() {
        return Err(DfmError::InvalidData(
            format!("failed to process some subdirectories or files {}: {:?}", context, errors)
        ));
    }
    Ok(found)
}

/// Report a periodic progress heartbeat during bulk analysis loops.
/// No-op unless the batch is large enough to be worth reporting,
/// so small, fast operations stay quiet.
///
/// Each update overwrites the single progress line in place (`\r`), and the
/// caller erases the line when the operation is done via `progress.clear()`.
/// Progress is written straight to stderr (not through the `log` crate) so it
/// is visible at every verbosity level, including `-v 0`.
pub(crate) fn report_progress(progress: &mut ProgressLine, done: usize, total: usize) {
    const BULK_PROGRESS_MIN: usize = 100;
    // Scale the step with the batch size so large operations still only
    // update the line about 20 times per run.
    let step = (total / 20).max(BULK_PROGRESS_MIN);
    if total >= BULK_PROGRESS_MIN && (done % step == 0 || done == total) {
        progress.set(&format!("processed {}/{} files", done, total));
    }
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
// Shared symlink-pointer / source-variant helpers
// ---------------------------------------------------------------------------

/// Read a symlink pointer file's content with surrounding whitespace trimmed.
pub(crate) fn read_symlink_pointer(pointer_file: &Path) -> Result<String, DfmError> {
    Ok(fs::read_to_string(pointer_file)?.trim().to_string())
}

/// Whether a symlink pointer file's (trimmed) content equals the pointee path.
pub(crate) fn symlink_pointer_matches(pointer_file: &Path, pointee: &str) -> Result<bool, DfmError> {
    Ok(read_symlink_pointer(pointer_file)? == pointee)
}

/// Which storage form a source file takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceVariant {
    Plain,
    Encrypted,
    Symlink,
}

/// Resolve the existing source counterpart(s) for `target_abs_path`, tried in
/// priority order: plain, encrypted, symlink pointer. Returns the first that
/// exists on disk, or `None` when the target has no source at all.
pub(crate) fn resolve_source_variant(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_abs_path: &PathBuf,
) -> Option<(SourceVariant, PathBuf)> {
    let plain = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, None,
    );
    if plain.exists() {
        return Some((SourceVariant::Plain, plain));
    }
    let encrypted = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path,
        Some(&settings.encrypted_postfix),
    );
    if encrypted.exists() {
        return Some((SourceVariant::Encrypted, encrypted));
    }
    let symlink = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path,
        Some(&settings.symlink_postfix),
    );
    if symlink.exists() {
        return Some((SourceVariant::Symlink, symlink));
    }
    None
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
/// Drop lines from an ignore file whose trimmed content makes `should_ignore`
/// return true. Blank lines and the full original text of kept lines are
/// preserved. A missing file is a no-op. When `dry_run` is true the file is not
/// written; the set of would-be-removed lines is still returned.
pub(crate) fn prune_ignore_file(
    ignore_file_path: &Path,
    should_ignore: impl Fn(&str) -> bool,
    dry_run: bool,
) -> Result<Vec<String>, DfmError> {
    if !ignore_file_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(ignore_file_path)?;
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || !should_ignore(&trimmed) {
            kept.push(line.to_string());
        } else {
            removed.push(trimmed.to_string());
        }
    }
    if !removed.is_empty() && !dry_run {
        fs::write(ignore_file_path, kept.join("\n"))?;
    }
    Ok(removed)
}

/// After a successful add/pull, drop the ignore patterns that were matched by
/// the affected files, so those files no longer remain ignored. No-op on
/// dry-run or when nothing matched.
pub(crate) fn prune_matched_ignore_patterns(
    xdg: &Xdg,
    patterns_to_remove: &[String],
    dry_run: bool,
) -> Result<(), DfmError> {
    if dry_run || patterns_to_remove.is_empty() {
        return Ok(());
    }
    let removed = prune_ignore_file(
        &calc_local_ignore_file(xdg)?,
        |t| patterns_to_remove.iter().any(|p| *p == t),
        dry_run,
    )?;
    info!("removed {} pattern(s) from ignore file", removed.len());
    Ok(())
}

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

/// RAII guard that removes a directory on drop, so temp dirs like
/// `.current_merge/` are cleaned up on every exit path (including `?` returns).
struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Run the merge tool inside `.current_merge/` in the source directory.
///
/// Creates temporary copies named `target.<file>` (working dir side),
/// `source.<file>` (cellar side, decrypted if encrypted) and an empty
/// `result.<file>` for the merge tool's output.  The merge tool must
/// write the merged result into `{result}` — after it succeeds the
/// result file is copied back to both the target and the source, and
/// the sync state is updated.
pub(crate) fn run_merge(
    settings: &Settings,
    source_abs_path: &PathBuf,
    target_abs_path: &PathBuf,
    state: &mut StateObject,
    source_dir_abs_path: &PathBuf,
) -> Result<(), DfmError> {
    let source_dir = PathBuf::from(&settings.source_dir);
    let merge_dir = source_dir.join(".current_merge");
    let _guard = DirGuard(merge_dir.clone());
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
        return Err(DfmError::Other("merge command is empty".into()));
    }
    let (prog, args) = parts.split_first().unwrap();
    let target_str = target_path.to_string_lossy();
    let source_str = source_path.to_string_lossy();
    let result_str = result_path.to_string_lossy();
    let args: Vec<String> = args.iter().map(|a| {

        a.replace("{target}", target_str.as_ref())
         .replace("{source}", source_str.as_ref())
         .replace("{result}", result_str.as_ref())
    }).collect();

    info!("running merge tool: {} {:?}", prog, args);

    let status = std::process::Command::new(prog)
        .args(&args)
        .status()
        .map_err(DfmError::Io)?;

    if !status.success() || !result_path.exists() {
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

    info!("merge completed for {:?}", target_abs_path);
    Ok(())
}
