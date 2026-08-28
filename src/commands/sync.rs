use std::fs;
use std::path::{Path, PathBuf};

use log::{debug, error, info, warn};
use regex::RegexSet;

use dfm::*;
use crate::DfmError;
use microxdg::Xdg;
use super::{
    sync_file_copy, read_symlink_pointer, resolve_source_variant,
    update_sync_state, get_sync_time, SourceVariant,
    list_directory_or_error, msg_dry_run, msg_nothing_to_do, msg_tasks_failure, report_progress,
    handle_ignore_or_override, IgnoreHandling, cli_path_in_scope,
};

/// Typed, per-command arguments for `sync` (built by the dispatcher).
pub struct SyncArgs {
    pub paths: Option<Vec<PathBuf>>,
    pub force: bool,
    pub dry_run: bool,
}

#[derive(Debug)]
enum SyncTask {
    /// Copy target → source (plain): mirror of `add`.
    Add(PathBuf, PathBuf),
    /// Copy source → target (plain): mirror of `pull`.
    Pull(PathBuf, PathBuf),
    /// Encrypt target → source (re-encrypt of an already-encrypted pair).
    AddEncrypted(PathBuf, PathBuf),
    /// Decrypt source → target.
    PullEncrypted(PathBuf, PathBuf),
    /// Update the source symlink pointer file to the target's current pointee.
    UpdatePointer(PathBuf, String),
}

/// Human-readable description of a task, shown before it runs (and during
/// --dry-run when it does not run).
fn describe_sync_task(task: &SyncTask) -> String {
    match task {
        SyncTask::Add(target, source) => format!("copy target {:?} to source {:?}", target, source),
        SyncTask::Pull(target, source) => format!("copy source {:?} to target {:?}", source, target),
        SyncTask::AddEncrypted(target, source) =>
            format!("copy encrypted target {:?} to source {:?}", target, source),
        SyncTask::PullEncrypted(target, source) =>
            format!("decrypt source {:?} to target {:?}", source, target),
        SyncTask::UpdatePointer(source_symlink, points_to) =>
            format!("directing source symlink file {:?} to the pointee {:?}", source_symlink, points_to),
    }
}

/// Handle a target entry that is a managed symlink (a source `.symlink`
/// pointer exists and has a sync record). The target symlink's pointee is the
/// truth the source pointer mirrors; when they diverge the pointer is updated.
#[allow(clippy::too_many_arguments)]
fn handle_symlink(
    settings: &Settings,
    target_dir_abs_path: &Path,
    source_dir_abs_path: &Path,
    target_path: &Path,
    target_ignore_regex: &RegexSet,
    target_ignore_file_path: &Path,
    state: &StateObject,
    tasks: &mut Vec<SyncTask>,
) -> Result<(), DfmError> {
    let symlink_rel = file_path_relative_to(target_path, target_dir_abs_path);
    // Ignored files are never processed; sync never edits the ignore file.
    if handle_ignore_or_override(
        target_ignore_regex, &symlink_rel, false,
        &mut Vec::new(), target_path, target_ignore_file_path,
    ) == IgnoreHandling::Skip {
        return Ok(());
    }

    let Some((SourceVariant::Symlink, source_symlink_file)) =
        resolve_source_variant(settings, target_dir_abs_path, source_dir_abs_path, target_path)
    else {
        debug!("target symlink {:?} has no managed source pointer, skipping", target_path);
        return Ok(());
    };

    // Only files with a sync record are eligible for sync; never-synced
    // symlinks (e.g. created after the last sync) are left untouched.
    if get_sync_time(state, &source_symlink_file, source_dir_abs_path).is_none() {
        debug!("source symlink file {:?} has no sync record, skipping", source_symlink_file);
        return Ok(());
    }

    let target_pointee = fs::read_link(target_path).map_err(|e| io_err(target_path, e))?;
    let target_pointee_str = target_pointee.to_string_lossy().into_owned();
    let source_content = read_symlink_pointer(&source_symlink_file)?;
    if source_content == target_pointee_str {
        debug!("target symlink {:?} and source pointer agree, up to date", target_path);
        return Ok(());
    }
    info!("target symlink {:?} points to {:?},\n\tpointer must be updated to {:?}",
        target_path, source_content, target_pointee_str);
    tasks.push(SyncTask::UpdatePointer(source_symlink_file, target_pointee_str));
    Ok(())
}

/// Handle a target entry that is a regular (non-symlink) file. Only files that
/// have a corresponding source *and* a sync record are eligible; everything
/// else (unmanaged, never-synced, unpulled) is skipped silently. Conflict
/// detection mirrors `add`/`pull`, and the copy is queued in the correct
/// direction (target→source for TargetModified, source→target for
/// SourceModified).
#[allow(clippy::too_many_arguments)]
fn handle_regular_file(
    settings: &Settings,
    target_dir_abs_path: &Path,
    source_dir_abs_path: &Path,
    target_path: &Path,
    target_ignore_regex: &RegexSet,
    target_ignore_file_path: &Path,
    internal_dfm_paths: &[PathBuf],
    state: &StateObject,
    tasks: &mut Vec<SyncTask>,
    conflict_detected: &mut bool,
    conflict_paths: &mut Vec<String>,
) -> Result<(), DfmError> {
    let target_abs_path = fs::canonicalize(target_path).map_err(|e| io_err(target_path, e))?;

    if target_abs_path.starts_with(source_dir_abs_path) {
        return Ok(());
    }
    if !target_abs_path.starts_with(target_dir_abs_path) {
        return Ok(());
    }
    if internal_dfm_paths.contains(&target_abs_path) {
        debug!("target {:?} is an internal dfm file, skipping", target_abs_path);
        return Ok(());
    }

    let target_rel = file_path_relative_to(&target_abs_path, target_dir_abs_path);
    // Ignored files are never processed, with or without --force: sync must
    // not override the ignore (which would also edit the ignore file).
    if handle_ignore_or_override(
        target_ignore_regex, &target_rel, false,
        &mut Vec::new(), &target_abs_path, target_ignore_file_path,
    ) == IgnoreHandling::Skip {
        return Ok(());
    }

    let Some((source_variant, source_abs_path)) =
        resolve_source_variant(settings, target_dir_abs_path, source_dir_abs_path, &target_abs_path)
    else {
        // Unmanaged target: no source side exists. sync never adds.
        debug!("target {:?} has no source, skipping (unmanaged)", target_abs_path);
        return Ok(());
    };

    // sync only processes files that have a sync record; a never-synced
    // file is left untouched (it is a candidate for add/pull instead).
    let Some(sync_time) = get_sync_time(state, &source_abs_path, source_dir_abs_path) else {
        debug!("source {:?} has no sync record, skipping (never-synced)", source_abs_path);
        return Ok(());
    };

    let cmp = compare_files(&settings.encrypted_postfix, &target_abs_path, &source_abs_path, Some(sync_time))?;

    match cmp {
        CompareByTimestamp::NonModified => {
            debug!("target {:?} and source {:?} are synchronized", target_abs_path, source_abs_path);
        },
        CompareByTimestamp::SourceModified => {
            match source_variant {
                SourceVariant::Encrypted => {
                    info!("only encrypted source {:?} was modified, decrypting to target", source_abs_path);
                    tasks.push(SyncTask::PullEncrypted(target_abs_path, source_abs_path));
                },
                _ => {
                    info!("only source {:?} was modified, copying to target", source_abs_path);
                    tasks.push(SyncTask::Pull(target_abs_path, source_abs_path));
                },
            }
        },
        CompareByTimestamp::TargetModified => {
            match source_variant {
                SourceVariant::Encrypted => {
                    info!("only target {:?} was modified, encrypting to source", target_abs_path);
                    tasks.push(SyncTask::AddEncrypted(target_abs_path, source_abs_path));
                },
                _ => {
                    info!("only target {:?} was modified, copying to source", target_abs_path);
                    tasks.push(SyncTask::Add(target_abs_path, source_abs_path));
                },
            }
        },
        CompareByTimestamp::BothModified => {
            info!("target {:?} and source {:?} were both modified, conflict", target_abs_path, source_abs_path);
            *conflict_detected = true;
            conflict_paths.push(target_path.to_string_lossy().into_owned());
        },
        CompareByTimestamp::NeverSynchronized => {
            // Unreachable in practice: a sync record exists by construction above.
            debug!("target {:?} and source {:?} were never synchronized, skipping", target_abs_path, source_abs_path);
        },
    }
    Ok(())
}

pub fn sync_command(settings: &Settings, xdg: &Xdg, args: SyncArgs, state: &mut StateObject) -> Result<(), DfmError> {
    let SyncArgs { ref paths, ref force, dry_run } = args;

    debug!("sync paths {:?}, force {}", paths, force);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(settings)?;

    let internal_dfm_paths: Vec<PathBuf> = [
        calc_state_file_path(xdg),
        calc_local_ignore_file(xdg),
    ].into_iter().filter_map(|r| r.ok()).collect();

    // Relative CLI paths are anchored at the target directory and may not
    // resolve out of the managed tree, same as add/pull.
    let paths = match paths {
        Some(p) => p.iter()
            .map(|p| cli_path_in_scope(p, &target_dir_abs_path, &source_dir_abs_path))
            .collect::<Result<Vec<_>, _>>()?,
        None => vec![target_dir_abs_path.clone()]
    };

    let target_ignore_file_path = calc_local_ignore_file(xdg)?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    let traversed_paths = list_directory_or_error(
        &paths,
        &target_dir_abs_path,
        Some(TraversalFilter::PruneIgnoredDirs(&target_ignore_regex)),
        "in targets",
    )?;

    let mut tasks: Vec<SyncTask> = Vec::new();
    let mut conflict_detected = false;
    let mut conflict_paths: Vec<String> = vec![];

    let mut progress = ProgressLine::new();
    for (i, target_path) in traversed_paths.iter().enumerate() {
        report_progress(&mut progress, i + 1, traversed_paths.len());
        debug!("checking {:?}", target_path);

        let handle = if target_path.is_symlink() {
            handle_symlink(
                settings, &target_dir_abs_path, &source_dir_abs_path, target_path,
                &target_ignore_regex, &target_ignore_file_path, state,
                &mut tasks,
            )
        } else {
            handle_regular_file(
                settings, &target_dir_abs_path, &source_dir_abs_path, target_path,
                &target_ignore_regex, &target_ignore_file_path, &internal_dfm_paths,
                state, &mut tasks, &mut conflict_detected, &mut conflict_paths,
            )
        };

        match handle {
            Ok(()) => {}
            Err(e) if e.is_permission_denied() => {
                warn_unreadable(target_path, &e);
            }
            Err(e) => return Err(e),
        }
    }
    progress.clear();

    // A conflict leaves the conflicting files untouched and blocks the whole
    // run unless --force is given. With --force the non-conflicting tasks still
    // run, but the conflicting files are never modified (they were never queued).
    if conflict_detected {
        if !*force {
            error!("sync conflicts detected for: {}", conflict_paths.join(", "));
            return Err(DfmError::Other(
                "sync conflicts detected: no files were modified".to_string(),
            ));
        }
        warn!("conflicts detected: {}; proceeding with --force on non-conflicting files",
            conflict_paths.join(", "));
    }

    if tasks.is_empty() {
        info!("{}", msg_nothing_to_do());
        return Ok(());
    }

    if dry_run {
        info!("{}", msg_dry_run());
    }

    debug!("::sync procedure begins, {} tasks", tasks.len());

    let total_tasks = tasks.len();
    let mut completed_tasks = 0usize;

    for task in tasks {
        info!("{}", describe_sync_task(&task));
        if dry_run {
            continue;
        }
        match execute_sync_task(&task, settings, state, &source_dir_abs_path) {
            Ok(completed) => {
                if completed {
                    completed_tasks += 1;
                }
            }
            Err(e) => {
                error!("{}", msg_tasks_failure(completed_tasks, total_tasks));
                return Err(e);
            }
        }
    }

    Ok(())
}

/// Run a single sync task against the filesystem. Returns `Ok(true)` when the
/// task completed, `Ok(false)` when it was skipped (unreadable path), and
/// `Err` on a hard failure that aborts the run.
fn execute_sync_task(
    task: &SyncTask,
    settings: &Settings,
    state: &mut StateObject,
    source_dir_abs_path: &Path,
) -> Result<bool, DfmError> {
    match task {
        SyncTask::Add(target_file, source_file) => {
            match sync_file_copy(target_file, source_file, source_file, state, source_dir_abs_path) {
                Ok(()) => Ok(true),
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(target_file, &e);
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        },
        SyncTask::Pull(target_file, source_file) => {
            match sync_file_copy(source_file, target_file, source_file, state, source_dir_abs_path) {
                Ok(()) => Ok(true),
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(target_file, &e);
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        },
        SyncTask::AddEncrypted(target_file, source_file) => {
            match dfm::crypt::write_encrypted_file(settings, target_file, source_file) {
                Ok(()) => {}
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(target_file, &e);
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
            update_sync_state(state, source_file, target_file, source_dir_abs_path)?;
            Ok(true)
        },
        SyncTask::PullEncrypted(target_file, source_file) => {
            match dfm::crypt::read_encrypted_file(settings, source_file, target_file) {
                Ok(()) => {}
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(target_file, &e);
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }
            update_sync_state(state, source_file, target_file, source_dir_abs_path)?;
            Ok(true)
        },
        SyncTask::UpdatePointer(source_symlink, points_to) => {
            let source_parent = source_symlink.parent()
                .ok_or_else(|| DfmError::Other(format!("cannot resolve parent directory of {:?}", source_symlink)))?;
            fs::create_dir_all(source_parent).map_err(|e| io_err(source_parent, e))?;
            fs::write(source_symlink, points_to.as_bytes()).map_err(|e| io_err(source_symlink, e))?;
            Ok(true)
        },
    }
}
