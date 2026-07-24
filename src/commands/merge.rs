use std::path::PathBuf;
use std::time::SystemTime;
use log::{debug, info, warn};
use dfm::*;
use crate::{Args, Command, DfmError};
use super::run_merge;

/// Convert a source-relative path (state key) to a target-relative path,
/// stripping encrypted/symlink postfixes just like pull.rs does.
fn source_rel_to_target_rel(source_rel: &str, settings: &Settings) -> String {
    let dot_prefix = &settings.dot_prefix;
    let mut target_rel = source_rel.replace(dot_prefix, ".");

    // Strip known postfixes (same order as pull.rs source-traversal branch)
    if target_rel.ends_with(&settings.symlink_postfix) {
        target_rel = target_rel[..target_rel.len() - settings.symlink_postfix.len()].to_string();
    } else if target_rel.ends_with(&settings.encrypted_postfix) {
        target_rel = target_rel[..target_rel.len() - settings.encrypted_postfix.len()].to_string();
    }

    target_rel
}

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

pub fn merge_command(settings: &Settings, args: &Args, state: &mut StateObject) -> Result<(), DfmError> {
    let Command::Merge { paths } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `merge`", args.command)));
    };

    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths(&settings)?;

    let target_ignore_file_path = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file_path)?;

    // Build list of (source_abs, target_abs, sync_time) tuples to check
    let mut candidates: Vec<(PathBuf, PathBuf, SystemTime)> = vec![];

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
                let target_rel = source_rel_to_target_rel(source_rel_str, settings);
                let inferred_target_abs = PathBuf::from_iter([target_dir_abs_path.to_str().unwrap(), &target_rel]);
                let inferred_target_abs = remove_dots_from_path(&inferred_target_abs);

                if let Some(sync_time) = state.syncs.get(source_rel_str) {
                    candidates.push((source_abs, inferred_target_abs, *sync_time));
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
                    let sync_time = state.syncs[&state_key];

                    candidates.push((source_abs, target_abs, sync_time));
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
            let target_rel = source_rel_to_target_rel(source_rel, settings);
            let target_abs = PathBuf::from_iter([target_dir_abs_path.to_str().unwrap(), &target_rel]);
            let target_abs = remove_dots_from_path(&target_abs);

            candidates.push((source_abs, target_abs, *sync_time));
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

        if let Some(pattern) = check_path_matches_regex(&target_ignore_regex, target_abs) {
            info!("target {:?} is ignored by regex /{}/ in file {:?}", target_abs, pattern, target_ignore_file_path);
            continue;
        }

        let cmp = compare_files_by_timestamps(target_abs, source_abs, Some(sync_time))?;
        if !matches!(cmp, CompareByTimestamp::BothModified) {
            debug!("{:?} is not BothModified, skipping", source_abs);
            continue;
        }

        warn!("both target {:?} and source {:?} were modified, merging...", target_abs, source_abs);
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
