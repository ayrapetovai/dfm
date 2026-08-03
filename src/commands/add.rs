use std::{env, fs};
use std::fs::File;
use std::io::Write;
use crate::DfmError;
use std::path::{Path, PathBuf};

use log::{debug, error, info, warn};
use regex::RegexSet;

use dfm::*;
use microxdg::Xdg;
use super::{sync_file_copy, require_force, symlink_pointer_matches,
            update_sync_state, remove_sync_state, get_sync_time,
            list_directory_or_error, msg_dry_run, msg_nothing_to_do, report_progress,
            prune_matched_ignore_patterns};

/// Typed, pre-resolved arguments for the `add` command (built by the
/// dispatcher from the matching clap subcommand).
pub struct AddArgs {
    pub paths: Option<Vec<PathBuf>>,
    pub force: bool,
    pub symlink: bool,
    pub encrypt: bool,
    pub dry_run: bool,
}

#[derive(Debug)]
enum AddTask {
    Copy(PathBuf, PathBuf),
    CopyEncryptedFile(PathBuf, PathBuf),
    CreateSymlinkFilePointer(PathBuf, PathBuf, String),
    CopyAndSymlink(PathBuf, PathBuf),
    UpdateSync(PathBuf, PathBuf),
}

/// Human-readable description of a task, shown before it runs (and during
/// --dry-run when it does not run).
fn describe_add_task(task: &AddTask) -> String {
    match task {
        AddTask::Copy(target, source) => format!("copy target {:?} to source {:?}", target, source),
        AddTask::CopyEncryptedFile(target, source) => format!("copy encrypted target {:?} to source {:?}", target, source),
        AddTask::CreateSymlinkFilePointer(source_symlink, _target, points_to) =>
            format!("directing source symlink file {:?} to the pointee of the target symlink {:?}", source_symlink, points_to),
        AddTask::CopyAndSymlink(target, source) => {
            format!("copy target {:?} to source {:?} and replace target with symlink", target, source)
        }
        AddTask::UpdateSync(source, _target) => format!("recording sync state for {:?}", source),
    }
}

/// Target path is a symlink. Resolve its pointee and point the matching source
/// symlink file at it. A fully-handled symlink never falls through to pointee
/// processing (the pointee is discovered independently by the traversal).
fn handle_target_symlink(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_path: &PathBuf,
    target_ignore_regex: &RegexSet,
    target_ignore_file_path: &PathBuf,
    encrypt: bool,
    force: bool,
    tasks: &mut Vec<AddTask>,
    error_messages: &mut Vec<String>,
) -> Result<(), DfmError> {
    if encrypt {
        error_messages.push(format!("Target {:?} is a symlink, encryption is impossible", target_path));
        return Ok(());
    }

    // `target_path` is absolute when it came from a traversal rooted at
    // `target_dir_abs`, but may be relative when the user named it explicitly
    // (e.g. `dfm add .bashrc`). Join with the current dir only in that case;
    // an absolute path must not be prefixed.
    let target_symlink_abs_path_raw = if target_path.is_absolute() {
        target_path.clone()
    } else {
        env::current_dir()?.join(target_path)
    };
    // canonicalize() would resolve the symlink itself, so canonicalize only its
    // parent directory and re-append the (still-symlink) name.
    let mut target_symlink_abs_path = {
        let root = PathBuf::from("/");
        fs::canonicalize(target_symlink_abs_path_raw.parent().get_or_insert(&root))?
    };
    target_symlink_abs_path.push(target_symlink_abs_path_raw.file_name()
        .ok_or_else(|| DfmError::InvalidInput("path has no file name".into()))?);

    let symlink_rel = file_path_relative_to(&target_symlink_abs_path, target_dir_abs_path);
    if let Some(pattern) = check_path_matches_regex_component_wise(target_ignore_regex, &symlink_rel) {
        info!("target symlink {:?} is ignored by regex /{}/ in file {:?}", target_symlink_abs_path, pattern, target_ignore_file_path);
        return Ok(());
    }

    let target_symlink_pointee_rel_path = fs::read_link(&target_symlink_abs_path)?;
    let target_symlink_pointee_abs_path = fs::canonicalize(&target_symlink_pointee_rel_path)
        .map_err(|e| DfmError::Other(format!(
            "Symlink {:?} points to {:?} which does not exist: {}",
            target_symlink_abs_path, target_symlink_pointee_rel_path, e
        )))?;
    debug!("target symlink {:?}\n\tpoints to {:?}", target_symlink_abs_path, target_symlink_pointee_abs_path);

    let source_symlink_file_abs_path = filepath_in_source_dir(
        &settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
        &target_symlink_abs_path, Some(&settings.symlink_postfix),
    );
    let source_symlink_file_exists = source_symlink_file_abs_path.exists();
    let target_pointee_rel_str = target_symlink_pointee_rel_path.to_string_lossy().into_owned();
    let source_symlink_file_points_to_right_target = if source_symlink_file_exists {
        symlink_pointer_matches(&source_symlink_file_abs_path, &target_pointee_rel_str)
            .unwrap_or(false)
    } else {
        false
    };

    if force || (source_symlink_file_exists && !source_symlink_file_points_to_right_target) {
        if !source_symlink_file_points_to_right_target {
            debug!("source symlink file points to the wrong file, must be {:?}", &target_symlink_pointee_rel_path);
        }
        tasks.push(AddTask::CreateSymlinkFilePointer(source_symlink_file_abs_path.clone(), target_symlink_abs_path.clone(), target_pointee_rel_str.clone()));
    } else if source_symlink_file_points_to_right_target {
        debug!("for target symlink {:?},\n\tsource symlink file {:?} already exists, skipping...", target_symlink_abs_path, source_symlink_file_abs_path);
    } else if !target_symlink_pointee_abs_path.starts_with(source_dir_abs_path) {
        debug!("for target symlink {:?},\n\tdoes not have a source symlink file {:?}", target_symlink_abs_path, source_symlink_file_abs_path);
        tasks.push(AddTask::CreateSymlinkFilePointer(source_symlink_file_abs_path.clone(), target_symlink_abs_path.clone(), target_pointee_rel_str));
    } else {
        debug!("target symlink {:?}\n\tpointee is managed as {:?}", source_symlink_file_abs_path, target_symlink_pointee_abs_path);
    }

    // Do NOT fall through to pointee processing — when walking the target
    // directory the pointee is discovered independently, and re-processing it
    // here would produce duplicate output for files already in state.
    debug!("target symlink {:?} points to {:?}, skipping pointee",
           target_symlink_abs_path, target_symlink_pointee_abs_path);
    Ok(())
}

/// Target path is a regular (non-symlink) file. Classify it (plain vs
/// encrypted), resolve the backing source, run the conflict check, and queue
/// the appropriate add task.
#[allow(clippy::too_many_arguments)]
fn handle_target_file(
    settings: &Settings,
    target_dir_abs_path: &PathBuf,
    source_dir_abs_path: &PathBuf,
    target_path: &Path,
    target_ignore_regex: &RegexSet,
    target_ignore_file_path: &PathBuf,
    internal_dfm_paths: &[PathBuf],
    encryption_regex_set: &RegexSet,
    symlink: bool,
    encrypt_flag: bool,
    force: bool,
    state: &StateObject,
    tasks: &mut Vec<AddTask>,
    error_messages: &mut Vec<String>,
    conflict_detected: &mut bool,
    patterns_to_remove: &mut Vec<String>,
) -> Result<(), DfmError> {
    let target_abs_path = fs::canonicalize(target_path)?;

    if target_abs_path.starts_with(source_dir_abs_path) {
        info!("target {:?} resides in source directory, ignoring", target_abs_path);
        return Ok(());
    }

    if !target_abs_path.starts_with(target_dir_abs_path) {
        info!("target {:?} does not reside in target directory {:?}, skipping...", target_abs_path, target_dir_abs_path);
        return Ok(());
    }

    // Skip internal dfm files (state, config, ignore) — the user should never
    // manage these via `dfm add`.
    if internal_dfm_paths.iter().any(|p| *p == target_abs_path) {
        debug!("target {:?} is an internal dfm file, skipping", target_abs_path);
        return Ok(());
    }

    let target_rel = file_path_relative_to(&target_abs_path, target_dir_abs_path);
    if let Some(pattern) = check_path_matches_regex_component_wise(target_ignore_regex, &target_rel) {
        if force {
            info!("target {:?} is ignored, --force overrides, will remove /{}/ from ignore file", target_abs_path, pattern);
            patterns_to_remove.push(pattern);
        } else {
            warn!("target {:?} is ignored by regex /{}/ in file {:?}", target_abs_path, pattern, target_ignore_file_path);
            return Ok(());
        }
    }

    let encrypt = if let Some(pattern) = check_path_matches_regex(encryption_regex_set, &target_abs_path) {
        debug!("target {:?} is forced to be encrypted by regex /{}/ from config file", target_abs_path, pattern);
        true
    } else {
        encrypt_flag
    };

    let encrypted_source_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, Some(&settings.encrypted_postfix));
    let regular_source_abs_path = filepath_in_source_dir(&settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path, &target_abs_path, None);

    let (source_is_encrypted, source_abs_path) = if encrypted_source_abs_path.exists() || encrypt {
        if regular_source_abs_path.exists() {
            // Converting from plain to encrypted. The plain source will be
            // deleted after encryption, so check if it has un-synced changes.
            let sync_time_opt = get_sync_time(state, &regular_source_abs_path, source_dir_abs_path);
            let cmp = compare_files(&settings.encrypted_postfix, &target_abs_path, &regular_source_abs_path, sync_time_opt)?;

            match cmp {
                CompareByTimestamp::BothModified => {
                    *conflict_detected = true;
                    if !force {
                        warn!("both target {:?} and plain source {:?} were modified, encryption would delete plain source changes",
                            target_abs_path, regular_source_abs_path);
                        return Ok(());
                    }
                },
                CompareByTimestamp::SourceModified => {
                    *conflict_detected = true;
                    if !force {
                        warn!("plain source {:?} was modified, encryption would discard those changes", regular_source_abs_path);
                        return Ok(());
                    }
                },
                CompareByTimestamp::NonModified => {
                    // safe to replace
                },
                CompareByTimestamp::TargetModified => {
                    // target is truth for add, safe to replace
                },
                CompareByTimestamp::NeverSynchronized => {
                    let content_equal = {
                        let t = fs::read_to_string(&target_abs_path)?;
                        let s = fs::read_to_string(&regular_source_abs_path)?;
                        t == s
                    };
                    if !(content_equal || force) {
                        warn!("plain source {:?}\n\tand target {:?}\n\tare different and were never synchronized.", regular_source_abs_path, target_abs_path);
                        warn!("Use --force to replace plain source with encrypted source");
                        return Ok(());
                    }
                },
            }
        }
        (true, encrypted_source_abs_path)
    } else {
        (false, regular_source_abs_path)
    };

    // check if a conflict could take place
    if source_abs_path.exists() {
        let sync_time_opt = get_sync_time(state, &source_abs_path, source_dir_abs_path);
        let cmp = compare_files(&settings.encrypted_postfix, &target_abs_path, &source_abs_path, sync_time_opt)?;

        match cmp {
            CompareByTimestamp::BothModified => {
                *conflict_detected = true;
                if !force {
                    warn!("both target {:?} and source {:?} were modified independently, `add` on this target will overwrite source",
                        target_abs_path, source_abs_path);
                    return Ok(());
                }
            },
            CompareByTimestamp::SourceModified => {
                *conflict_detected = true;
                if !force {
                    warn!("source {:?} was modified, `add`ing the target {:?} will overwrite changes in source",
                        source_abs_path, target_abs_path);
                    return Ok(());
                }
            },
            CompareByTimestamp::NonModified => {
                debug!("neither target nor source were modified");
                if !force {
                    return Ok(());
                }
            },
            CompareByTimestamp::TargetModified => {
                info!("only target {:?} was modified, no conflicts", target_abs_path);
            },
            CompareByTimestamp::NeverSynchronized => {
                let content_equal = {
                    let t = fs::read_to_string(&target_abs_path)?;
                    let s = fs::read_to_string(&source_abs_path)?;
                    t == s
                };
                if content_equal {
                    debug!("target and source are identical, recording sync state");
                    tasks.push(AddTask::UpdateSync(source_abs_path.clone(), target_abs_path.clone()));
                    return Ok(());
                }
                *conflict_detected = true;
                if !force {
                    warn!("target {:?}\n\tand source {:?}\n\tare different and were never synchronized. Use --force to overwrite", target_abs_path, source_abs_path);
                    return Ok(());
                }
            },
        }

        debug!("no conflict detected for target {:?}", target_abs_path);
    } else {
        info!("source file {:?} does not exist", source_abs_path);
    }

    if symlink && (encrypt || source_is_encrypted) {
        error_messages.push(format!("Target {:?} is encrypted but --symlink was requested", target_abs_path));
    } else if encrypt || source_is_encrypted {
        tasks.push(AddTask::CopyEncryptedFile(target_abs_path, source_abs_path));
    } else if symlink {
        tasks.push(AddTask::CopyAndSymlink(target_abs_path, source_abs_path));
    } else {
        tasks.push(AddTask::Copy(target_abs_path, source_abs_path));
    }
    Ok(())
}

pub fn add_command(settings: &Settings, xdg: &Xdg, args: AddArgs, state: &mut StateObject) -> Result<(), DfmError> {
    let AddArgs { ref paths, ref force, ref symlink, ref encrypt, dry_run } = args;

    debug!("add paths {:?}, force {}, symlink {}, encrypt {}", paths, force, symlink, encrypt);

    if *symlink && *encrypt {
        return Err(DfmError::other("--symlink and --encrypt are mutually exclusive"));
    }

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;

    // Compute internal dfm file paths so they can be excluded from traversal.
    // These files (state, config, target-ignore) are dfm's own files — the user
    // should never manage them via `add`.
    let internal_dfm_paths: Vec<PathBuf> = [
        calc_state_file_path(xdg),
        calc_config_file_path(xdg),
        calc_local_ignore_file(xdg),
    ].into_iter().filter_map(|r| r.ok()).collect();

    let paths = match paths {
        Some(p) => p.clone(),
        None => vec![target_dir_abs_path.clone()]
    };

    let target_ignore_file_path = calc_local_ignore_file(xdg)?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;
    // Compiled once per command, not per file: building force-encryption
    // patterns inside the traversal loop was O(files × patterns) compaction work.
    let encryption_regex_set = RegexSet::new(
        settings.force_encryption_for.iter().map(|r| r.as_str().to_owned())
    )?;

    let traversed_paths = list_directory_or_error(
        &paths,
        &target_dir_abs_path,
        Some(TraversalFilter::PruneIgnoredDirs(&target_ignore_regex)),
        "in targets",
    )?;
    debug!("traversing result is {:?}", traversed_paths);

    let mut tasks: Vec<AddTask> = Vec::new();

    debug!("::check state procedure begins");

    let mut conflict_detected = false;
    let mut error_messages = vec![];
    let mut patterns_to_remove: Vec<String> = vec![];

    let mut progress = ProgressLine::new();
    for (i, target_path) in traversed_paths.iter().enumerate() {
        report_progress(&mut progress, i + 1, traversed_paths.len());
        debug!("checking {:?}", target_path);

        if target_path.is_symlink() {
            handle_target_symlink(
                &settings, &target_dir_abs_path, &source_dir_abs_path, target_path,
                &target_ignore_regex, &target_ignore_file_path,
                *encrypt, *force, &mut tasks, &mut error_messages,
            )?;
        } else {
            handle_target_file(
                &settings, &target_dir_abs_path, &source_dir_abs_path, target_path,
                &target_ignore_regex, &target_ignore_file_path,
                &internal_dfm_paths, &encryption_regex_set,
                *symlink, *encrypt, *force, state,
                &mut tasks, &mut error_messages, &mut conflict_detected, &mut patterns_to_remove,
            )?;
        }
    }
    progress.clear();

    if !error_messages.is_empty() {
        let joined = format!("add failed: {}", error_messages.join("; "));
        error!("{}", joined);
        require_force(*force, joined)?;
    }

    if conflict_detected {
        require_force(*force, "conflicts")?;
        warn!("conflicts detected, proceeding with --force");
    }

    if tasks.is_empty() {
        info!("{}", msg_nothing_to_do());
        return Ok(());
    }

    if dry_run {
        info!("{}", msg_dry_run());
    }

    debug!("::copy procedure begins, {} tasks", tasks.len());

    for task in tasks {
        // Print what each task would do even under --dry-run.
        info!("{}", describe_add_task(&task));
        if dry_run {
            continue;
        }
        match task {
            AddTask::Copy(target_file, source_file) => {
                sync_file_copy(&target_file, &source_file, &source_file, state, &source_dir_abs_path)?;
            },
            AddTask::CopyAndSymlink(target_file, source_file) => {
                // 1. Copy file content to source
                sync_file_copy(&target_file, &source_file, &source_file, state, &source_dir_abs_path)?;

                // 2. Remove the original target file
                fs::remove_file(&target_file)?;

                // 3. Create a symlink at the target pointing to the source file
                let target_parent = target_file.parent()
                    .ok_or_else(|| DfmError::other("target file has no parent directory"))?
                    .to_path_buf();
                let link_target = file_path_relative_to(&source_file, &target_parent);
                symlink::symlink_file(&link_target, &target_file)?;
            },
            AddTask::CopyEncryptedFile(target_file, source_file) => {
                dfm::crypt::write_zip_file(settings, &target_file, &source_file)?;

                update_sync_state(state, &source_file, &target_file, &source_dir_abs_path)?;

                // If a plain source exists, remove it — replaced by the encrypted version
                let plain_source = filepath_in_source_dir(
                    &settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                    &target_file, None,
                );
                if plain_source.exists() {
                    fs::remove_file(&plain_source)?;
                    // Remove stale state entry for the plain source
                    remove_sync_state(state, &plain_source, &source_dir_abs_path);
                }
            },
            AddTask::CreateSymlinkFilePointer(source_symlink, target_abs, points_to) => {
                // open if exists or create, if it doesn't
                let mut symlink_file = File::create(&source_symlink)?;
                symlink_file.write_all(points_to.as_bytes())?;

                update_sync_state(state, &source_symlink, &target_abs, &source_dir_abs_path)?;
            },
            AddTask::UpdateSync(source_file, target_file) => {
                update_sync_state(state, &source_file, &target_file, &source_dir_abs_path)?;
            },
        }
    }

    prune_matched_ignore_patterns(xdg, &patterns_to_remove, dry_run)?;

    Ok(())
}
