use std::fs;
use std::path::PathBuf;

use log::{debug, error, info, warn};
use regex::RegexSet;
use walkdir::WalkDir;

use dfm::*;
use crate::DfmError;
use super::{sync_file_copy, require_force, read_symlink_pointer,
            update_sync_state, get_sync_time,
            list_directory_or_error, msg_dry_run, msg_nothing_to_do, report_progress,
            prune_matched_ignore_patterns, handle_ignore_or_override, IgnoreHandling};
use microxdg::Xdg;

/// Typed, per-command arguments for `pull` (built by the dispatcher).
pub struct PullArgs {
    pub paths: Option<Vec<PathBuf>>,
    pub force: bool,
    pub symlink: bool,
    pub dry_run: bool,
}

/// Keep any-depth file whose name does not start with `.` (dotfiles pruned).
const PULL_KEEP_NON_DOTFILES: &str = r#"^(.+/)?[^.][^/]+$"#;

#[derive(Debug)]
enum PullTask {
    Copy(PathBuf, PathBuf),
    CreateOrUpdateSymlink(PathBuf, String),
    Decrypt(PathBuf, PathBuf),
}

/// Human-readable description of a task, shown before it runs (and during
/// --dry-run when it does not run).
fn describe_pull_task(task: &PullTask) -> String {
    match task {
        PullTask::Copy(target, source) => format!("copy source {:?}\n\tto target {:?}", source, target),
        PullTask::CreateOrUpdateSymlink(target, points_to) => {
            format!("create symlink {:?} pointing\n\tto {:?}", target, points_to)
        }
        PullTask::Decrypt(target, source) => format!("decrypt source {:?}\n\tto target {:?}", source, target),
    }
}

/// Handle the encrypted-source timestamp comparison for pull.
/// Pushes a `Decrypt` task or returns an error via `require_force`.
fn handle_encrypted_timestamps(
    cmp: CompareByTimestamp,
    source_abs: &PathBuf,
    target_abs: &PathBuf,
    force: bool,
    tasks: &mut Vec<PullTask>,
) -> Result<(), DfmError> {
    match cmp {
        CompareByTimestamp::BothModified => {
            warn!("both target and encrypted source {:?} were modified, merge needed", source_abs);
            require_force(force, "target and encrypted source have conflicting modifications")?;
            tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
        },
        CompareByTimestamp::NonModified => {
            if force {
                info!("force flag set, decrypting despite no modifications");
                tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
            } else {
                info!("neither target nor encrypted source were modified, no action needed, skipping...");
            }
        },
        CompareByTimestamp::TargetModified => {
            warn!("target was modified, pulling encrypted source will overwrite those changes");
            require_force(force, "target was modified")?;
            tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
        },
        CompareByTimestamp::SourceModified => {
            info!("only the encrypted source was modified, decrypting...");
            tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
        },
        CompareByTimestamp::NeverSynchronized => {
            warn!("target {:?}\n\tand encrypted source {:?}\n\twere not synchronized.", target_abs, source_abs);
            require_force(force, "encrypted source was not synchronized")?;
            tasks.push(PullTask::Decrypt(target_abs.clone(), source_abs.clone()));
        },
    }
    Ok(())
}

/// Handle a source-side path (the walked entry lies inside the source
/// directory). Returns `None` when the path was fully handled — a task was
/// queued or the entry was skipped — and `Some(target_file_abs_path)` to fall
/// through to regular target processing below.
fn handle_source_path(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    source_file_abs_path: &PathBuf,
    target_ignore_regex: &RegexSet,
    target_ignore_file_path: &PathBuf,
    target_must_be_symlink: bool,
    force: bool,
    state: &StateObject,
    tasks: &mut Vec<PullTask>,
    patterns_to_remove: &mut Vec<String>,
) -> Result<Option<PathBuf>, DfmError> {
    debug!("provided path of a source {:?}", source_file_abs_path);

    let source_name = source_file_abs_path.to_string_lossy().into_owned();
    let source_rel_str = file_path_relative_to(source_file_abs_path, source_dir_abs_path).to_string_lossy().into_owned();
    let target_file_rel_to_target_dir = source_rel_to_target_rel(
        &source_rel_str, &settings.dot_prefix,
        &settings.symlink_postfix, &settings.encrypted_postfix,
    );
    let target_file_abs_path = remove_dots_from_path(&target_dir_abs_path.join(&target_file_rel_to_target_dir));
    debug!("inferred target {:?}", target_file_abs_path);

    if handle_ignore_or_override(
        target_ignore_regex, &PathBuf::from(&target_file_rel_to_target_dir), force,
        patterns_to_remove, &target_file_abs_path, target_ignore_file_path,
    ) == IgnoreHandling::Skip {
        return Ok(None);
    }

    if !target_file_abs_path.exists() && source_file_abs_path.exists() {
        if source_name.ends_with(&settings.symlink_postfix) {
            let source_file_content = read_symlink_pointer(source_file_abs_path)?;
            debug!("source is a symlink file, pointing to {}", source_file_content);
            tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
            return Ok(None);
        } else if source_name.ends_with(&settings.encrypted_postfix) {
            debug!("decrypting source {:?}\n\tto target {:?}", source_file_abs_path, target_file_abs_path);
            tasks.push(PullTask::Decrypt(target_file_abs_path, source_file_abs_path.clone()));
            return Ok(None);
        } else {
            if target_must_be_symlink {
                debug!("symlink creating task");
                tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path.clone(), source_file_abs_path.to_string_lossy().into_owned()));
            } else {
                debug!("regular file creating task");
                tasks.push(PullTask::Copy(target_file_abs_path, source_file_abs_path.clone()));
            }
            return Ok(None);
        }
    } else if target_file_abs_path.is_symlink() && source_file_abs_path.exists() && source_name.ends_with(&settings.symlink_postfix) {
        // Only a `.symlink` source file carries a pointer; a plain source file
        // next to a symlink target is the `add -s` layout and must not have its
        // content misread as a symlink pointer.
        let target_symlink_pointee = fs::read_link(&target_file_abs_path)
            .map_err(|e| io_err(&target_file_abs_path, e))?;
        let source_file_content = read_symlink_pointer(source_file_abs_path)?;
        if source_file_content != target_symlink_pointee.to_string_lossy().as_ref() {
            info!("target symlink {:?} points to {:?},\n\tmust point to {:?}", target_file_abs_path, target_symlink_pointee, source_file_content);
            tasks.push(PullTask::CreateOrUpdateSymlink(target_file_abs_path, source_file_content));
            return Ok(None);
        }
    } else if target_file_abs_path.exists() && source_name.ends_with(&settings.encrypted_postfix) {
        debug!("target {:?} exists, source is encrypted, checking timestamps", target_file_abs_path);

        let cmp = compare_files(
            &settings.encrypted_postfix, &target_file_abs_path, source_file_abs_path,
            get_sync_time(state, source_file_abs_path, source_dir_abs_path),
        )?;

        handle_encrypted_timestamps(cmp, source_file_abs_path, &target_file_abs_path, force, tasks)?;
        return Ok(None);
    }
    // TODO check if the pointee of the symlink also is under management and needs to be pulled.
    Ok(Some(target_file_abs_path))
}

/// Handle a path resolved to the target directory. Applies the ignore check
/// and dispatches to the existing-target or missing-target scenario.
fn handle_target_path(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_abs_path: &PathBuf,
    target_must_be_symlink: bool,
    force: bool,
    state: &StateObject,
    target_ignore_regex: &RegexSet,
    target_ignore_file_path: &PathBuf,
    tasks: &mut Vec<PullTask>,
    patterns_to_remove: &mut Vec<String>,
    error_list: &mut Vec<String>,
) -> Result<(), DfmError> {
    let target_rel_path = file_path_relative_to(target_abs_path, target_dir_abs_path);
    if handle_ignore_or_override(
        target_ignore_regex, &target_rel_path, force,
        patterns_to_remove, target_abs_path, target_ignore_file_path,
    ) == IgnoreHandling::Skip {
        return Ok(());
    }

    if target_abs_path.exists() {
        handle_existing_target(
            settings, target_dir_abs_path, source_dir_abs_path, target_abs_path, force, state, tasks, error_list,
        )
    } else {
        handle_missing_target(
            settings, target_dir_abs_path, source_dir_abs_path, target_abs_path, target_must_be_symlink, tasks,
        )
    }
}

/// Target path exists. Queue a symlink fix-up, an encrypted decrypt, or a
/// plain copy, applying the conflict check for regular files.
fn handle_existing_target(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_abs_path: &PathBuf,
    force: bool,
    state: &StateObject,
    tasks: &mut Vec<PullTask>,
    error_list: &mut Vec<String>,
) -> Result<(), DfmError> {
    if target_abs_path.is_symlink() {
        let target_symlink_followed_abs_path = fs::canonicalize(target_abs_path)
            .map_err(|e| DfmError::Other(format!(
                "Target symlink {:?} is broken (points to non-existent path): {}",
                target_abs_path, e
            )))?;

        let source_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, None);
        if target_symlink_followed_abs_path == source_file_abs_path {
            info!("target symlink {:?}\n\tpoints to the source file {:?}, skipping...", target_abs_path, source_file_abs_path);
            return Ok(());
        }

        let source_symlink_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, Some(&settings.symlink_postfix));
        if source_symlink_file_abs_path.exists() {
            let target_symlink_pointee_path = fs::read_link(target_abs_path)
                .map_err(|e| io_err(target_abs_path, e))?;
            let source_file_content = read_symlink_pointer(&source_symlink_file_abs_path)?;
            if source_file_content == target_symlink_pointee_path.to_string_lossy().as_ref() {
                info!("target symlink {:?}\n\tpoints to {}, skipping...", target_abs_path, target_symlink_pointee_path.to_string_lossy());
                return Ok(());
            } else {
                info!("target symlink {:?}\n\tpoints to {},\n\tmust point to {:?}", target_abs_path, target_symlink_pointee_path.to_string_lossy(), source_file_content);
                tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
                return Ok(());
            }
        } else {
            if !target_symlink_followed_abs_path.starts_with(source_dir_abs_path) {
                info!("target symlink {:?} does not point to the source directory, skipping...", target_abs_path);
                // TODO remove the symlink?
                return Ok(());
            }
        }

        // also the case is handled when the symlink points inside the source directory but
        // to the wrong file
        tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_abs_path.to_string_lossy().into_owned()));
        return Ok(());
    }

    // existing target file is not a symlink
    let source_abs_path = filepath_in_source_dir(&settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, None);
    if !source_abs_path.exists() {
        // Check for encrypted source before giving up
        let source_encrypted_abs_path = filepath_in_source_dir(&settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, Some(&settings.encrypted_postfix));
        if source_encrypted_abs_path.exists() {
            debug!("target {:?} exists, encrypted source found, checking timestamps", target_abs_path);

            let cmp = compare_files(
                &settings.encrypted_postfix, target_abs_path, &source_encrypted_abs_path,
                get_sync_time(state, &source_encrypted_abs_path, source_dir_abs_path),
            )?;

            handle_encrypted_timestamps(cmp, &source_encrypted_abs_path, target_abs_path, force, tasks)?;
            return Ok(());
        }
        info!("target {:?} is unmanaged,\n\tno source {:?} found, skipping...", target_abs_path, source_abs_path);
        return Ok(());
    }

    let sync_time_opt = get_sync_time(state, &source_abs_path, source_dir_abs_path);
    let cmp = compare_files(&settings.encrypted_postfix, target_abs_path, &source_abs_path, sync_time_opt)?;

    match cmp {
        CompareByTimestamp::BothModified => {
            warn!("both source and target were modified, merge needed");
            require_force(force, "target and source have conflicting modifications")?;
        },
        CompareByTimestamp::NonModified => {
            if force {
                info!("force flag set, copying despite no modifications");
            } else {
                info!("both source and target were not modified, no action needed, skipping...");
                return Ok(());
            }
        },
        CompareByTimestamp::TargetModified => {
            warn!("target was modified, pulling source will overwrite those changes");
            require_force(force, "target was modified")?;
        },
        CompareByTimestamp::SourceModified => {
            info!("only the source was modified")
        },
        CompareByTimestamp::NeverSynchronized => {
            warn!("target {:?}\n\tand source {:?}\n\twere not synchronized.", target_abs_path, source_abs_path);
            error_list.push(format!("target {:?} and source {:?} were not synchronized", target_abs_path, source_abs_path));
            if !force {
                return Ok(());
            }
        },
    }
    tasks.push(PullTask::Copy(target_abs_path.clone(), source_abs_path));
    Ok(())
}

/// Target path does not exist. Queue a copy / decrypt / symlink task from the
/// matching source variant, or error when no source exists.
fn handle_missing_target(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_abs_path: &PathBuf,
    target_must_be_symlink: bool,
    tasks: &mut Vec<PullTask>,
) -> Result<(), DfmError> {
    debug!("target {:?} does not exist", target_abs_path);

    let source_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, None);
    if source_file_abs_path.exists() {
        if source_file_abs_path.is_dir() {
            // Source is a directory — walk it recursively to find all files
            // and create individual pull tasks for each.
            for entry in WalkDir::new(&source_file_abs_path)
                .follow_links(false)
                .follow_root_links(false)
            {
                let entry = entry.map_err(|e| {
                    let msg = e.to_string();
                    let inner = e.into_io_error()
                        .unwrap_or_else(|| std::io::Error::other(msg));
                    DfmError::Io(inner)
                })?;
                if entry.file_type().is_dir() {
                    continue;
                }
                let source_file = entry.path().to_path_buf();
                let relative = source_file
                    .strip_prefix(&source_file_abs_path)
                    .map_err(|e| DfmError::Other(e.to_string()))?;
                let target_file = target_abs_path.join(relative);
                info!("source {:?} will be copied\n\tto the target {:?}", source_file, target_file);
                if target_must_be_symlink {
                    tasks.push(PullTask::CreateOrUpdateSymlink(target_file, source_file.to_string_lossy().into_owned()));
                } else {
                    tasks.push(PullTask::Copy(target_file, source_file));
                }
            }
            return Ok(());
        }

        info!("source {:?} will be copied\n\tto the target {:?}", source_file_abs_path, target_abs_path);
        if target_must_be_symlink {
            tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_abs_path.to_string_lossy().into_owned()));
        } else {
            tasks.push(PullTask::Copy(target_abs_path.clone(), source_file_abs_path));
        }
        return Ok(());
    }

    let source_encrypted_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, Some(&settings.encrypted_postfix));
    if source_encrypted_file_abs_path.exists() {
        info!("encrypted source {:?} will be decrypted\n\tto the target {:?}", source_encrypted_file_abs_path, target_abs_path);
        tasks.push(PullTask::Decrypt(target_abs_path.clone(), source_encrypted_file_abs_path));
        return Ok(());
    }

    let source_symlink_file_abs_path = filepath_in_source_dir(&settings.dot_prefix, target_dir_abs_path, source_dir_abs_path, target_abs_path, Some(&settings.symlink_postfix));
    if source_symlink_file_abs_path.exists() {
        info!("source symlink file {:?} will be used to create a target symlink", source_symlink_file_abs_path);
        let source_file_content = read_symlink_pointer(&source_symlink_file_abs_path)?;
        tasks.push(PullTask::CreateOrUpdateSymlink(target_abs_path.clone(), source_file_content));
        return Ok(());
    }

    Err(DfmError::NotFound(
        format!("for target {:?} no corresponding source file found", target_abs_path)
    ))
}

pub fn pull_command(settings: &Settings, xdg: &Xdg, args: PullArgs, state: &mut StateObject) -> Result<(), DfmError> {
    let PullArgs { ref paths, ref force, symlink: ref target_must_be_symlink, dry_run } = args;

    debug!("pull paths {:?}, force {}, dry-run {}", paths, force, dry_run);

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(settings)?;

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![source_dir_abs_path.clone()]
    };

    let regex_no_dot_files = RegexSet::new(vec![PULL_KEEP_NON_DOTFILES]).unwrap();
    let traversed_paths = list_directory_or_error(
        &paths,
        &source_dir_abs_path,
        Some(TraversalFilter::KeepMatching(&regex_no_dot_files)),
        "in source",
    )?;
    debug!("traversing result is {:?}", traversed_paths);


    let target_ignore_file_path = calc_local_ignore_file(xdg)?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    let mut tasks: Vec<PullTask> = vec![];
    let mut error_list = vec![];
    let mut patterns_to_remove: Vec<String> = vec![];

    let mut progress = ProgressLine::new();
    for (i, path) in traversed_paths.iter().enumerate() {
        report_progress(&mut progress, i + 1, traversed_paths.len());
        debug!("checking {:?}", path);

        let target_abs_path = remove_dots_from_path(&target_dir_abs_path.join(path));

        // Source-path scenario: the walked entry lies inside the source dir.
        let target_abs_path = if target_abs_path.starts_with(&source_dir_abs_path) {
            match handle_source_path(
                settings, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path,
                &target_ignore_regex, &target_ignore_file_path,
                *target_must_be_symlink, *force, state, &mut tasks, &mut patterns_to_remove,
            ) {
                Ok(Some(target)) => target,
                Ok(None) => continue,
                Err(e) if e.is_permission_denied() => {
                    warn!("skipping unreadable path {:?}: {}", target_abs_path, e);
                    continue;
                }
                Err(e) => return Err(e),
            }
        } else {
            target_abs_path
        };

        // Regular target-path processing.
        match handle_target_path(
            settings, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path,
            *target_must_be_symlink, *force, state, &target_ignore_regex,
            &target_ignore_file_path, &mut tasks, &mut patterns_to_remove, &mut error_list,
        ) {
            Ok(()) => {}
            Err(e) if e.is_permission_denied() => {
                warn!("skipping unreadable path {:?}: {}", target_abs_path, e);
            }
            Err(e) => return Err(e),
        }
    }
    progress.clear();

    if !error_list.is_empty() {
        require_force(*force, "improper operation")?;
    }

    if tasks.is_empty() {
        info!("{}", msg_nothing_to_do());
        return Ok(());
    }

    if dry_run {
        info!("{}", msg_dry_run());
    }

    debug!("::copy procedure begins, {} tasks", tasks.len());

    let total_tasks = tasks.len();
    let mut completed_tasks = 0usize;

    for task in tasks.iter() {
        // Print what each task would do even under --dry-run.
        info!("{}", describe_pull_task(task));
        if dry_run {
            continue;
        }
        match execute_pull_task(task, settings, state, &source_dir_abs_path) {
            Ok(completed) => {
                if completed {
                    completed_tasks += 1;
                }
            }
            Err(e) => {
                error!(
                    "{} of {} tasks completed before failure",
                    completed_tasks, total_tasks
                );
                return Err(e);
            }
        }
    }

    prune_matched_ignore_patterns(xdg, &patterns_to_remove, dry_run)?;

    Ok(())
}

/// Run a single pull task against the filesystem. Returns `Ok(true)` when the
/// task completed, `Ok(false)` when it was skipped (unreadable path, e.g.
/// permission denied), and `Err` on a hard failure that aborts the run.
fn execute_pull_task(
    task: &PullTask,
    settings: &Settings,
    state: &mut StateObject,
    source_dir_abs_path: &PathBuf,
) -> Result<bool, DfmError> {
    match task {
        PullTask::Copy(target_file, source_file) => {
            match sync_file_copy(source_file, target_file, source_file, state, source_dir_abs_path) {
                Ok(()) => Ok(true),
                Err(e) if e.is_permission_denied() => {
                    warn!("skipping unreadable path {:?}: {}", target_file, e);
                    Ok(false)
                }
                Err(e) => Err(e),
            }
        },
        PullTask::CreateOrUpdateSymlink(target_symlink_file_path, points_to) => {
            if let Err(e) = symlink::remove_symlink_file(target_symlink_file_path) {
                match e.kind() {
                    std::io::ErrorKind::NotFound => {
                        info!("target symlink {:?} does not exist", target_symlink_file_path);
                        // is ok
                    },
                    _ => return Err(io_err(target_symlink_file_path, e)),
                }
            }
            let points_to = if points_to.starts_with("./") {
                &points_to[2..]
            } else {
                points_to.as_str()
            };
            let pointee = PathBuf::from(points_to);

            match symlink::symlink_file(pointee, target_symlink_file_path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    warn!("skipping unreadable path {:?}: {}", target_symlink_file_path, e);
                    return Ok(false);
                }
                Err(e) => return Err(io_err(target_symlink_file_path, e)),
            }
            debug!("target symlink {:?} updated", target_symlink_file_path);
            Ok(true)
        },
        PullTask::Decrypt(target_file, source_file) => {
            match dfm::crypt::read_encrypted_file(settings, source_file, target_file) {
                Ok(()) => {}
                Err(e) if e.is_permission_denied() => {
                    warn!("skipping unreadable path {:?}: {}", target_file, e);
                    return Ok(false);
                }
                Err(e) => return Err(e),
            }

            update_sync_state(state, source_file, target_file, source_dir_abs_path)?;
            Ok(true)
        },
    }
}
