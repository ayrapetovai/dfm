use std::path::PathBuf;
use log::{debug, info, warn};
use dfm::*;
use crate::{Args, Command, DfmError};
use microxdg::Xdg;
use super::{run_merge, source_rel_to_target_rel, resolve_dry_run, msg_dry_run};

/// Look up a source-relative path in state, trying known postfixes.
/// Returns the matching state key (which may include encrypted/symlink postfix).
fn resolve_state_key(
    state: &StateObject,
    base_rel: &str,
    encrypted_postfix: &str,
    symlink_postfix: &str,
) -> Option<String> {
    // Try exact match first
    if state.syncs.contains_key(base_rel) {
        return Some(base_rel.to_string());
    }
    // Try with encrypted postfix
    let enc = format!("{}{}", base_rel, encrypted_postfix);
    if state.syncs.contains_key(&enc) {
        return Some(enc);
    }
    // Try with symlink postfix
    let sym = format!("{}{}", base_rel, symlink_postfix);
    if state.syncs.contains_key(&sym) {
        return Some(sym);
    }
    None
}

pub fn merge_command(settings: &Settings, xdg: &Xdg, args: &Args, state: &mut StateObject) -> Result<(), DfmError> {
    let Command::Merge { paths, dry_run } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `merge`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);
    let paths_provided = paths.is_some();

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;

    if dry_run {
        info!("{}", msg_dry_run());
    }

    let target_ignore_file_path = calc_local_ignore_file(xdg)?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    // Build list of (source_abs, target_abs, sync_time) tuples to check
    let mut candidates: Vec<(PathBuf, PathBuf, SyncTime)> = vec![];

    if let Some(paths) = paths {
        for path in paths {
            let target_abs = if path.is_relative() {
                let base = std::env::current_dir()?;
                PathBuf::from_iter([base, path.clone()])
            } else {
                path.clone()
            };
            let target_abs = remove_dots_from_path(&target_abs);

            if target_abs.starts_with(&source_dir_abs_path) {
                // Provided path is in the source directory — infer target
                // (same logic as pull.rs source-traversal branch)
                let source_abs = target_abs;
                let source_rel = file_path_relative_to(&source_abs, &source_dir_abs_path);
                let source_rel = remove_dots_from_path(&source_rel);
                let source_rel_str = source_rel.to_str().unwrap();

                // Derive target path (replace dot_prefix, strip postfixes)
                let target_rel = source_rel_to_target_rel(
                    source_rel_str, &settings.dot_prefix,
                    &settings.symlink_postfix, &settings.encrypted_postfix,
                );
                let inferred_target_abs = PathBuf::from_iter([target_dir_abs_path.to_str().unwrap(), &target_rel]);
                let inferred_target_abs = remove_dots_from_path(&inferred_target_abs);

                if let Some(sync_time) = state.syncs.get(source_rel_str) {
                    candidates.push((source_abs, inferred_target_abs, sync_time.clone()));
                } else {
                    warn!("{:?} is not in the state file, skipping...", source_rel);
                }
            } else {
                // Provided path is a target path — find source
                let source_abs_base = filepath_in_source_dir(
                    &settings.dot_prefix, &target_dir_abs_path, &source_dir_abs_path,
                    &target_abs, None,
                );
                let source_rel_base = file_path_relative_to(&source_abs_base, &source_dir_abs_path);
                let source_rel_base = remove_dots_from_path(&source_rel_base);
                let source_rel_base_str = source_rel_base.to_str().unwrap();

                // State key may include encrypted/symlink postfix — try all variants
                let state_key = resolve_state_key(
                    state,
                    source_rel_base_str,
                    &settings.encrypted_postfix,
                    &settings.symlink_postfix,
                );

                if let Some(state_key) = state_key {
                    let source_abs = if state_key == source_rel_base_str {
                        source_abs_base
                    } else {
                        PathBuf::from_iter([source_dir_abs_path.to_str().unwrap(), &state_key])
                    };
                    let source_abs = remove_dots_from_path(&source_abs);
                    let sync_time = &state.syncs[&state_key];

                    candidates.push((source_abs, target_abs, sync_time.clone()));
                } else {
                    warn!("{:?} is not in the state file, skipping...", target_abs);
                }
            }
        }
    } else {
        // No paths given — iterate over all state entries
        for (source_rel, sync_time) in &state.syncs {
            let source_abs = PathBuf::from_iter([source_dir_abs_path.to_str().unwrap(), source_rel]);
            let source_abs = remove_dots_from_path(&source_abs);

            // Derive target path (replace dot_prefix, strip postfixes)
            let target_rel = source_rel_to_target_rel(
                source_rel, &settings.dot_prefix,
                &settings.symlink_postfix, &settings.encrypted_postfix,
            );
            let target_abs = PathBuf::from_iter([target_dir_abs_path.to_str().unwrap(), &target_rel]);
            let target_abs = remove_dots_from_path(&target_abs);

            candidates.push((source_abs, target_abs, sync_time.clone()));
        }
    }

    if candidates.is_empty() {
        info!("nothing to merge — no conflicting files found");
        return Ok(());
    }

    let mut merged_count = 0;
    for (source_abs, target_abs, sync_time) in &candidates {
        if !source_abs.exists() || !target_abs.exists() {
            debug!("source {:?} or target {:?} does not exist, skipping", source_abs, target_abs);
            continue;
        }

        if target_abs.is_symlink() {
            debug!("target {:?} is a symlink (managed via symlink), skipping merge", target_abs);
            continue;
        }

        let target_rel = file_path_relative_to(target_abs, &target_dir_abs_path);
        if let Some(pattern) = check_path_matches_regex_component_wise(&target_ignore_regex, &target_rel) {
            info!("target {:?} is ignored by regex /{}/ in file {:?}", target_abs, pattern, target_ignore_file_path);
            continue;
        }

        let cmp = compare_files(&settings.encrypted_postfix, target_abs, source_abs, Some(sync_time))?;
        if !matches!(cmp, CompareByTimestamp::BothModified) && !paths_provided {
            debug!("{:?} is not BothModified, skipping", source_abs);
            continue;
        }

        warn!("both target {:?} and source {:?} were modified, merging...", target_abs, source_abs);
        if dry_run {
            info!("would merge {:?} (dry run)", target_abs);
            merged_count += 1;
            continue;
        }
        run_merge(settings, source_abs, target_abs, state, &source_dir_abs_path)?;
        info!("merged {:?}", target_abs);
        merged_count += 1;
    }

    if merged_count == 0 {
        info!("no conflicting files to merge");
    } else {
        info!("merged {} file(s)", merged_count);
    }

    Ok(())
}
