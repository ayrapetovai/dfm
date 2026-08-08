use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use log::{debug, error, info, warn};

use dfm::*;
use crate::DfmError;
use microxdg::Xdg;
use super::{require_force, get_sync_time, read_symlink_pointer,
            resolve_source_variant, SourceVariant, state_key_for,
            list_directory_or_error,
            msg_dry_run, msg_nothing_to_do, report_progress};

/// Typed, per-command arguments for `forget` (built by the dispatcher).
pub struct ForgetArgs {
    pub paths: Option<Vec<PathBuf>>,
    pub force: bool,
    pub dry_run: bool,
}

/// A single queued `forget` mutation, executed after analysis finishes.
enum ForgetTask {
    Delete(PathBuf),
    RemoveState(String),
}

fn source_to_state_key(source_abs: &PathBuf, source_dir_abs: &PathBuf) -> String {
    state_key_for(source_abs, source_dir_abs)
}

/// Remove a source object from disk. A regular file or symlink is removed with
/// `remove_file`; a directory (e.g. the whole subtree of a forgotten target
/// directory) is removed with `remove_dir_all`.
fn remove_source_object(source_file: &Path) -> io::Result<()> {
    match fs::symlink_metadata(source_file) {
        Ok(meta) if meta.file_type().is_dir() => fs::remove_dir_all(source_file),
        _ => fs::remove_file(source_file),
    }
}

/// Target path is a symlink. Queue deletions for the pointer file and/or the
/// target symlink. Returns `true` when the entry was fully handled (the loop
/// should move on) and `false` when the symlink has no pointer file (the
/// loop falls through to pointee processing).
fn handle_target_symlink(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_path: &PathBuf,
    force: bool,
    tasks: &mut Vec<ForgetTask>,
) -> Result<bool, DfmError> {
    let target_abs_path = remove_dots_from_path(&target_dir_abs_path.join(target_path));
    let target_symlink_pointee_path = fs::read_link(&target_abs_path)
        .map_err(|e| io_err(&target_abs_path, e))?;

    debug!("target symlink {:?}\n\tpoints to {:?}", target_abs_path, target_symlink_pointee_path);
    if target_symlink_pointee_path.starts_with(source_dir_abs_path) {
        info!("target symlink {:?}\n\tpoints into source directory, removing", target_abs_path);
        tasks.push(ForgetTask::Delete(target_abs_path.clone()));
    }

    let source_symlink_file_abs_path = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs_path, source_dir_abs_path,
        &target_abs_path, Some(&settings.symlink_postfix)
    );
    if !source_symlink_file_abs_path.exists() {
        debug!("symlink {:?}\n\tdoes not have source symlink file {:?}, skipping...", target_abs_path, source_symlink_file_abs_path);
        return Ok(false);
    }

    handle_symlink_pointer(
        &target_abs_path,
        &target_symlink_pointee_path,
        &source_symlink_file_abs_path,
        force,
        tasks,
    )?;
    Ok(true)
}

/// Target path no longer exists on disk. Queue deletion of the source file,
/// resolved either directly (a relative path mirroring the source layout) or
/// through the plain/encrypted/symlink variants.
fn handle_missing_target(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_path: &PathBuf,
    tasks: &mut Vec<ForgetTask>,
) -> Result<(), DfmError> {
    let direct_source = source_dir_abs_path.join(target_path);
    if direct_source.exists() {
        info!("source {:?} will be removed", direct_source);
        tasks.push(ForgetTask::Delete(direct_source));
        return Ok(());
    }

    let target_abs_path = remove_dots_from_path(&target_dir_abs_path.join(target_path));
    let Some((_variant, source_abs_path)) = resolve_source_variant(
        settings, target_dir_abs_path, source_dir_abs_path, &target_abs_path,
    ) else {
        info!("source for {:?} does not exist, skipping...", target_path);
        return Ok(());
    };

    info!("source {:?} will be removed", source_abs_path);
    tasks.push(ForgetTask::Delete(source_abs_path));
    Ok(())
}

/// Target path resolved into the source directory (a source-side path).
/// Queues deletion of the source file or symlink pointer.
fn handle_source_path(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_abs_path: &PathBuf,
    force: bool,
    tasks: &mut Vec<ForgetTask>,
) -> Result<(), DfmError> {
    if !target_abs_path.to_string_lossy().ends_with(&settings.symlink_postfix) {
        info!("source {:?} will be removed", target_abs_path);
        tasks.push(ForgetTask::Delete(target_abs_path.clone()));
        return Ok(());
    }

    // A source symlink pointer file — remove it unless the target symlink
    // it manages still exists and points elsewhere.
    let source_rel_str = file_path_relative_to(target_abs_path, source_dir_abs_path).to_string_lossy().into_owned();
    let target_rel_str = source_rel_to_target_rel(
        &source_rel_str, &settings.dot_prefix,
        &settings.symlink_postfix, &settings.encrypted_postfix,
    );
    let target_symlink_abs_path = target_dir_abs_path.join(&target_rel_str);
    if !target_symlink_abs_path.exists() {
        return Ok(());
    }

    let target_symlink_pointee_path = fs::read_link(&target_symlink_abs_path)
        .map_err(|e| io_err(&target_symlink_abs_path, e))?;
    handle_symlink_pointer(
        &target_symlink_abs_path,
        &target_symlink_pointee_path,
        target_abs_path,
        force,
        tasks,
    )?;
    Ok(())
}

/// A managed symlink whose pointer file matches its current pointee is
/// up-to-date: forget just deletes the pointer. When they diverge, the
/// pointer is only deleted with `--force`.
fn handle_symlink_pointer(
    target_symlink_abs_path: &Path,
    target_symlink_pointee_path: &Path,
    source_file: &Path,
    force: bool,
    tasks: &mut Vec<ForgetTask>,
) -> Result<(), DfmError> {
    let source_file_content = read_symlink_pointer(source_file)?;
    if source_file_content == target_symlink_pointee_path.to_string_lossy().as_ref() {
        info!(
            "target symlink {:?}\n\tpoints to {}, skipping...",
            target_symlink_abs_path,
            target_symlink_pointee_path.to_string_lossy()
        );
        tasks.push(ForgetTask::Delete(source_file.to_path_buf()));
    } else {
        info!(
            "target symlink {:?}\n\tpoints to {},\n\tmust point to {:?}",
            target_symlink_abs_path,
            target_symlink_pointee_path.to_string_lossy(),
            source_file_content
        );
        if force {
            tasks.push(ForgetTask::Delete(source_file.to_path_buf()));
        } else {
            info!("specify --force to delete source {:?}", source_file);
        }
    }
    Ok(())
}

/// Target path is a regular file inside the target directory. Queue deletion
/// of its plain/encrypted source, subject to the conflict check.
fn handle_target_file(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_abs_path: &PathBuf,
    force: bool,
    state: &StateObject,
    tasks: &mut Vec<ForgetTask>,
    error_messages: &mut Vec<String>,
) -> Result<(), DfmError> {
    let Some((variant, source_abs_path)) = resolve_source_variant(
        settings, target_dir_abs_path, source_dir_abs_path, target_abs_path,
    ) else {
        info!("source for {:?} does not exist, skipping...", target_abs_path);
        return Ok(());
    };
    // A regular target file is never backed by a symlink pointer; treat an
    // orphan pointer as "no source".
    if variant == SourceVariant::Symlink {
        info!("source for {:?} does not exist, skipping...", target_abs_path);
        return Ok(());
    }

    // A directory in the source is a container of managed files, not a file
    // itself. Forgetting its target forgets the whole subtree.
    if source_abs_path.is_dir() {
        info!("source {:?} is a directory, removing its whole subtree", source_abs_path);
        tasks.push(ForgetTask::Delete(source_abs_path.clone()));
        return Ok(());
    }

    let sync_time_opt = get_sync_time(state, &source_abs_path, source_dir_abs_path);
    let cmp = compare_files(&settings.encrypted_postfix, target_abs_path, &source_abs_path, sync_time_opt)?;

    if cmp == CompareByTimestamp::SourceModified {
        if force {
            info!("source {:?} was modified, removing source", source_abs_path);
            tasks.push(ForgetTask::Delete(source_abs_path.clone()));
        } else {
            warn!("source {:?} was modified, use --force to remove", source_abs_path);
            error_messages.push("source was modified".into());
        }
        return Ok(());
    }
    if cmp == CompareByTimestamp::BothModified {
        if force {
            info!("source {:?} and target {:?} were both modified, removing source", source_abs_path, target_abs_path);
            tasks.push(ForgetTask::Delete(source_abs_path.clone()));
        } else {
            warn!("source {:?} and target {:?} were both modified, use --force to remove", source_abs_path, target_abs_path);
            error_messages.push("source and target were modified".into());
        }
        return Ok(());
    }
    if cmp == CompareByTimestamp::TargetModified {
        if force {
            info!("target {:?} was modified, removing source", target_abs_path);
            tasks.push(ForgetTask::Delete(source_abs_path.clone()));
        } else {
            warn!("target {:?} was modified, use --force to remove", target_abs_path);
            error_messages.push("target was modified".into());
        }
        return Ok(());
    }

    info!("source {:?} will be removed", source_abs_path);
    tasks.push(ForgetTask::Delete(source_abs_path));
    Ok(())
}

pub fn forget_command(settings: &Settings, xdg: &Xdg, args: ForgetArgs, state: &mut StateObject) -> Result<(), DfmError> {
    let ForgetArgs { ref paths, ref force, dry_run } = args;

    debug!("forget paths {:?}, force {}, dry-run {}", paths, force, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(settings)?;

    let forget_all = paths.is_none();
    let paths = match paths {
        Some(p) => p.clone(),
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
    debug!("traversing result is {:?}", traversed_paths);

    let mut tasks: Vec<ForgetTask> = Vec::new();
    let mut error_messages: Vec<String> = vec![];

    debug!("::check state procedure begins");

    let mut progress = ProgressLine::new();
    for (i, target_path) in traversed_paths.iter().enumerate() {
        report_progress(&mut progress, i + 1, traversed_paths.len());
        debug!("checking {:?}", target_path);

        // Symlink scenario — fully handled when a pointer file exists; a
        // symlink with no pointer file falls through to pointee processing.
        if target_path.is_symlink() {
            let symlink_handled = match handle_target_symlink(
                settings, &target_dir_abs_path, &source_dir_abs_path, target_path, *force, &mut tasks,
            ) {
                Ok(v) => v,
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(target_path, &e);
                    continue;
                }
                Err(e) => return Err(e),
            };
            if symlink_handled {
                continue;
            }
        }

        let target_abs_path = match fs::canonicalize(target_path) {
            Ok(abs) => abs,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                warn_unreadable(target_path, &e);
                continue;
            }
            Err(e) => {
                if target_path.is_symlink() {
                    debug!("symlink {:?} is broken: {}", target_path, e);
                } else {
                    handle_missing_target(
                        settings, &target_dir_abs_path, &source_dir_abs_path, target_path, &mut tasks,
                    )?;
                }
                continue;
            }
        };

        if target_abs_path.starts_with(&source_dir_abs_path) {
            match handle_source_path(
                settings, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, *force, &mut tasks,
            ) {
                Ok(()) => {}
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(&target_abs_path, &e);
                }
                Err(e) => return Err(e),
            }
        } else if target_abs_path.starts_with(&target_dir_abs_path) {
            match handle_target_file(
                settings, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path,
                *force, state, &mut tasks, &mut error_messages,
            ) {
                Ok(()) => {}
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(&target_abs_path, &e);
                }
                Err(e) => return Err(e),
            }
        } else {
            warn!("target {:?}\n\tresides outside the target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
        }
    }
    progress.clear();

    // Build set of state keys already covered by tasks
    let mut processed_keys: HashSet<String> = HashSet::new();
    for task in &tasks {
        match task {
            ForgetTask::Delete(source_abs) => {
                processed_keys.insert(source_to_state_key(source_abs, &source_dir_abs_path));
            },
            ForgetTask::RemoveState(key) => {
                processed_keys.insert(key.clone());
            },
        }
    }

    // Process state entries not covered by the traversal (orphaned / unpulled).
    // Only run when no explicit paths were given ("forget all" mode).
    let orphan_keys: Vec<String> = if forget_all {
        state.syncs.keys()
            .filter(|k| !processed_keys.contains(k.as_str()))
            .cloned()
            .collect()
    } else {
        vec![]
    };
    let orphan_total = orphan_keys.len();
    for (i, key) in orphan_keys.into_iter().enumerate() {
        report_progress(&mut progress, i + 1, orphan_total);
        let source_abs = source_dir_abs_path.join(&key);
        let source_abs = remove_dots_from_path(&source_abs);

        if source_abs.exists() {
            let sync_time = &state.syncs[&key];
            let source_meta = source_abs.metadata().map_err(|e| io_err(&source_abs, e))?;
            let source_mtime = source_meta.modified().map_err(|e| io_err(&source_abs, e))?;
            if source_mtime > sync_time.mtime {
                if *force {
                    info!("source {:?} was modified, removing with --force", key);
                    tasks.push(ForgetTask::Delete(source_abs));
                } else {
                    warn!("source {:?} was modified, use --force to remove", key);
                    error_messages.push(format!("source {:?} was modified", key));
                }
            } else {
                info!("source {:?} will be removed", source_abs);
                tasks.push(ForgetTask::Delete(source_abs));
            }
        } else {
            info!("source for {:?} does not exist, removing state entry", key);
            tasks.push(ForgetTask::RemoveState(key));
        }
    }
    progress.clear();

    if !error_messages.is_empty() {
        let joined = format!("forget failed: {}", error_messages.join("; "));
        error!("{}", joined);
        require_force(*force, joined)?;
    }

    if tasks.is_empty() {
        info!("{}", msg_nothing_to_do());
        return Ok(());
    }

    if dry_run {
        info!("{}", msg_dry_run());
    }

    debug!("::remove procedure begins, {} tasks", tasks.len());

    // Phase 1: Delete source files (best-effort, never abort mid-phase)
    let mut delete_errors: Vec<(String, String)> = Vec::new();
    for task in &tasks {
        let ForgetTask::Delete(source_file) = task else { continue; };
        info!("delete {:?}", source_file);
        if dry_run {
            continue;
        }
        if let Err(e) = remove_source_object(source_file) {
            if e.kind() == std::io::ErrorKind::NotFound {
                debug!("{:?} was already removed, skipping", source_file);
            } else {
                warn!("failed to delete {:?}: {}", source_file, e);
                delete_errors.push((source_file.to_string_lossy().into_owned(), e.to_string()));
            }
        }
    }

    // Phase 2: Remove state entries for all processed files (infallible).
    // Skipped under --dry-run: a dry-run must not mutate the state file.
    for task in &tasks {
        if dry_run {
            continue;
        }
        match task {
            ForgetTask::Delete(source_file) => {
                // A deleted directory removes the whole subtree, so clear every
                // state entry that lives at that source path or beneath it.
                let key = source_to_state_key(source_file, &source_dir_abs_path);
                let key_prefix = format!("{}/", key);
                state.syncs.retain(|k, _| k != &key && !k.starts_with(&key_prefix));
            },
            ForgetTask::RemoveState(key) => {
                state.syncs.remove(key);
            },
        }
    }

    // Phase 3: Clean up empty parent directories (best-effort)
    for task in &tasks {
        let ForgetTask::Delete(source_file) = task else { continue; };
        if dry_run {
            continue;
        }
        let mut parent_opt = source_file.parent();
        while let Some(dir) = parent_opt {
            parent_opt = dir.parent();
            if dir != source_dir_abs_path && dir.starts_with(&source_dir_abs_path) {
                let mut read_dir_entries = match dir.read_dir() {
                    Ok(entries) => entries,
                    Err(_) => break,
                };
                if read_dir_entries.next().is_none() {
                    if let Err(e) = fs::remove_dir(dir) {
                        warn!("failed to remove empty directory {:?}: {}", dir, e);
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    if !delete_errors.is_empty() {
        error!("some source files could not be deleted: {:?}", delete_errors);
        let summary: String = delete_errors.iter()
            .map(|(path, err)| format!("{}: {}", path, err))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(DfmError::Other(format!("failed to delete source files: {}", summary)));
    }

    Ok(())
}
