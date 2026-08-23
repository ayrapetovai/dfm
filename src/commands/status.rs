use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCmd, Stdio};
use std::io::{IsTerminal, Write};

use colored::Colorize;
use log::{debug, info};
use regex::RegexSet;

use dfm::*;
use crate::DfmError;
use microxdg::Xdg;
use super::{list_directory, report_progress, split_command, state_key_for, source_rel_to_target_abs};

/// Typed, per-command arguments for `status` (built by the dispatcher).
pub struct StatusArgs {
    pub all: bool,
    pub short: bool,
    pub porcelain: bool,
    pub conflicted: bool,
    pub modified: bool,
    pub unmanaged: bool,
    pub managed: bool,
    pub unpulled: bool,
    pub ignored: bool,
    pub ignored_patterns: bool,
    pub unused_patterns: bool,
    /// Restrict the report to these paths (absolute or relative to the target
    /// directory). `None` shows the full report over the whole target dir.
    pub paths: Option<Vec<PathBuf>>,
}

// Types

/// Two-letter status code. The `Display` output is part of the CLI contract
/// (`--porcelain` is stable, machine-readable) and must stay byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusCode {
    UpToDate,
    BothModified,
    TargetModified,
    SourceModified,
    NeverSynchronized,
    Unpulled,
    Unmanaged,
    UnmanagedSymlink,
    ManagedSymlink,
    Ignored,
    IgnoredSymlink,
    StalePattern,
}

impl StatusCode {
    fn is_modified(self) -> bool {
        matches!(self, StatusCode::BothModified | StatusCode::TargetModified | StatusCode::SourceModified)
    }

    fn is_managed(self) -> bool {
        matches!(
            self,
            StatusCode::UpToDate
                | StatusCode::BothModified
                | StatusCode::TargetModified
                | StatusCode::SourceModified
                | StatusCode::NeverSynchronized
                | StatusCode::Unpulled
                | StatusCode::ManagedSymlink
        )
    }

    fn is_ignored(self) -> bool {
        matches!(self, StatusCode::Ignored | StatusCode::IgnoredSymlink)
    }

    fn is_up_to_date(self) -> bool {
        matches!(self, StatusCode::UpToDate | StatusCode::ManagedSymlink)
    }
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let code = match self {
            StatusCode::UpToDate => "--",
            StatusCode::BothModified => "MM",
            StatusCode::TargetModified => "M ",
            StatusCode::SourceModified => " M",
            StatusCode::NeverSynchronized => "NM",
            StatusCode::Unpulled => "!?",
            StatusCode::Unmanaged => "??",
            StatusCode::UnmanagedSymlink => "?L",
            StatusCode::ManagedSymlink => "LL",
            StatusCode::Ignored => "!!",
            StatusCode::IgnoredSymlink => "!L",
            StatusCode::StalePattern => "!P",
        };
        f.write_str(code)
    }
}

#[derive(Debug)]
struct StatusEntry {
    /// Two-letter status code.
    code: StatusCode,
    /// Display path (relative to target or source directory).
    path: String,
    /// Ignore pattern that matched this file (only for `!!` entries).
    matched_pattern: Option<String>,
}

// Status command entry point

/// Resolve user-provided status paths (absolute or relative to the target
/// directory) to absolute roots for traversal. With `None`, the whole target
/// directory is used. A path that does not exist is an error so the user gets
/// immediate feedback instead of an empty report.
///
/// A path that lies inside the source directory is treated as a source path:
/// it is mapped back to its target counterpart (like `pull` does) so the user
/// can naturally ask about a managed file by either of its two locations.
fn resolve_status_paths(
    paths: Option<&Vec<PathBuf>>,
    target_dir_abs: &Path,
    source_dir_abs: &Path,
    settings: &Settings,
) -> Result<Vec<PathBuf>, DfmError> {
    match paths {
        None => Ok(vec![target_dir_abs.to_path_buf()]),
        Some(paths) => {
            let mut roots = Vec::with_capacity(paths.len());
            for p in paths {
                let abs = if p.is_absolute() {
                    remove_dots_from_path(p)
                } else {
                    remove_dots_from_path(&target_dir_abs.join(p))
                };
                if !abs.exists() {
                    return Err(DfmError::other(format!(
                        "path does not exist: {}",
                        abs.display()
                    )));
                }
                // A source-dir path maps to the target file that it manages:
                // `dot_files/...` -> `target/...` with postfixes stripped.
                let root = if abs.starts_with(source_dir_abs) {
                    let source_rel = file_path_relative_to(&abs, source_dir_abs);
                    let (_, target_abs) = source_rel_to_target_abs(
                        &source_rel.to_string_lossy(),
                        target_dir_abs,
                        settings,
                    );
                    target_abs
                } else {
                    abs
                };
                roots.push(root);
            }
            Ok(roots)
        }
    }
}

pub fn status_command(settings: &Settings, xdg: &Xdg, args: StatusArgs, state: &StateObject) -> Result<(), DfmError> {
    let StatusArgs {
        ref all,
        ref short,
        ref porcelain,
        ref conflicted,
        ref modified,
        ref unmanaged,
        ref managed,
        ref unpulled,
        ref ignored,
        ref ignored_patterns,
        ref unused_patterns,
        ref paths,
    } = args;

    let (target_dir_abs, source_dir_abs) = calc_working_dir_paths(settings)?;

    let target_ignore_file = calc_local_ignore_file(xdg)?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file)?;

    // Restrict the report to the requested paths (absolute or relative to the
    // target directory). With no paths, the whole target dir is analyzed.
    let requested_roots = resolve_status_paths(paths.as_ref(), &target_dir_abs, &source_dir_abs, settings)?;

    // Paths to dfm's own internal files (skip in unmanaged detection)
    let state_file_path = calc_state_file_path(xdg).ok();
    let config_file_path = calc_config_file_path(xdg).ok();

    // ------------------------------------------------------------------
    // Phase 1 — Process every state entry (managed files)
    // ------------------------------------------------------------------
    let mut entries: Vec<StatusEntry> = Vec::new();
    let mut state_keys: HashSet<String> = HashSet::new();

    let mut progress = ProgressLine::new();
    for (i, (source_rel, sync_time)) in state.syncs.iter().enumerate() {
        report_progress(&mut progress, i + 1, state.syncs.len());
        state_keys.insert(source_rel.clone());

        let source_abs = source_dir_abs.join(source_rel);
        let source_abs = remove_dots_from_path(&source_abs);

        let target_rel = source_rel_to_target_rel(
            source_rel,
            &settings.dot_prefix,
            &settings.symlink_postfix,
            &settings.encrypted_postfix,
        );
        let target_abs = target_dir_abs.join(&target_rel);
        let target_abs = remove_dots_from_path(&target_abs);

        // Keep `state_keys` fully populated (it drives Phase-2 classification),
        // but only emit entries for paths within the requested scope.
        if !requested_roots.iter().any(|root| target_abs.starts_with(root)) {
            continue;
        }

        debug!("status: state entry {:?} → target {:?}", source_rel, target_abs);

        // Check if this is a managed symlink (state key ends with symlink_postfix)
        let is_managed_symlink = source_rel.ends_with(&settings.symlink_postfix);

        // Check ignore patterns
        if let Some(pattern) = check_path_matches_regex_component_wise(&target_ignore_regex, &PathBuf::from(&target_rel)) {
            let code = if is_managed_symlink { StatusCode::IgnoredSymlink } else { StatusCode::Ignored };
            entries.push(StatusEntry {
                code,
                path: target_rel.clone(),
                matched_pattern: Some(pattern),
            });
            continue;
        }

        // Classify
        let target_exists = target_abs.exists();
        let source_exists = source_abs.exists();

        if is_managed_symlink {
            // Managed symlink: present if source pointer file exists
            if !source_exists {
                state_keys.remove(source_rel);
                debug!("status: stale state entry {:?}, source symlink missing", source_rel);
                continue;
            }
            let code = if target_exists {
                StatusCode::ManagedSymlink
            } else {
                StatusCode::Unpulled
            };
            entries.push(StatusEntry { code, path: target_rel.clone(), matched_pattern: None });
            continue;
        }

        // Regular file classification via timestamp comparison
        let (code, path) = if !target_exists && source_exists {
            (StatusCode::Unpulled, target_rel.clone())
        } else if target_exists && !source_exists {
            state_keys.remove(source_rel);
            debug!("status: stale state entry {:?}, source missing", source_rel);
            continue;
        } else if target_exists && source_exists {
            let cmp = match compare_files(&settings.encrypted_postfix, &target_abs, &source_abs, Some(sync_time)) {
                Ok(cmp) => cmp,
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(&target_abs, &e);
                    continue;
                }
                Err(e) => return Err(e),
            };
            match cmp {
                CompareByTimestamp::BothModified => (StatusCode::BothModified, target_rel.clone()),
                CompareByTimestamp::TargetModified => (StatusCode::TargetModified, target_rel.clone()),
                CompareByTimestamp::SourceModified => (StatusCode::SourceModified, target_rel.clone()),
                CompareByTimestamp::NonModified => (StatusCode::UpToDate, target_rel.clone()),
                CompareByTimestamp::NeverSynchronized => (StatusCode::NeverSynchronized, target_rel.clone()),
            }
        } else {
            state_keys.remove(source_rel);
            debug!("status: stale state entry {:?}, both sides missing", source_rel);
            continue;
        };

        entries.push(StatusEntry {
            code,
            path,
            matched_pattern: None,
        });
    }
    progress.clear();

    // ------------------------------------------------------------------
    // Phase 2 — Walk target directory for unmanaged files
    // ------------------------------------------------------------------
    let ListDirectories { found: traversed_target, errors: traversal_errors, pruned: pruned_dirs } =
        list_directory(
            &requested_roots,
            &target_dir_abs,
            Some(TraversalFilter::PruneIgnoredDirs(&target_ignore_regex)),
        )?;
    if !traversal_errors.is_empty() {
        return Err(DfmError::InvalidData(format!(
            "failed to process some subdirectories or files in target directory for status: {:?}",
            traversal_errors
        )));
    }

    // Phase 3 builds its own list from traversed_target + pruned dirs + entries

    // Pre-compute canonical source dir for robust path comparison
    let canon_source_dir = fs::canonicalize(&source_dir_abs).unwrap_or_else(|_| source_dir_abs.clone());

    for (i, target_abs) in traversed_target.iter().enumerate() {
        report_progress(&mut progress, i + 1, traversed_target.len());
        // Skip files inside the source directory — normalize via canonicalize
        // to avoid path-comparison edge cases (symlinks, double slashes, etc.)
        if let Ok(canon_target) = fs::canonicalize(target_abs) {
            if canon_target.starts_with(&canon_source_dir) {
                continue;
            }
        } else {
            // If canonicalize fails (e.g. broken symlink), fall back to string compare
            if target_abs.starts_with(&source_dir_abs) {
                continue;
            }
        }

        // Skip known dfm internal files (state, config, ignore)
        if let Some(ref sfp) = state_file_path
            && *target_abs == *sfp
        {
            continue;
        }
        if let Some(ref cfp) = config_file_path
            && *target_abs == *cfp
        {
            continue;
        }
        if *target_abs == target_ignore_file {
            continue;
        }

        // Compute the relative path for the display
        let rel_str = state_key_for(target_abs, &target_dir_abs);

        if target_abs.is_symlink() {
            classify_target_symlink(
                settings, &target_dir_abs, &source_dir_abs, &target_ignore_regex,
                target_abs, &rel_str, &state_keys, *all, *ignored, &mut entries,
            );
        } else {
            classify_target_file(
                settings, &target_dir_abs, &source_dir_abs, &target_ignore_regex,
                target_abs, &rel_str, &state_keys, *all, *ignored, &mut entries,
            );
        }
    }
    progress.clear();

    // Entries for fully-ignored directories that were pruned during the walk:
    // one `!! dir/` per directory instead of enumerating every file inside it.
    for pruned_rel in &pruned_dirs {
        let matched_pattern = dir_ignore_pattern(&target_ignore_regex, pruned_rel);
        entries.push(StatusEntry {
            code: StatusCode::Ignored,
            path: format!("{}/", pruned_rel),
            matched_pattern,
        });
    }

    // ------------------------------------------------------------------
    // Phase 3 — Find unused ignore patterns
    // ------------------------------------------------------------------
    let mut stale_patterns: Vec<String> = Vec::new();

    // Unused-pattern detection is a full-tree analysis: a pattern is "unused"
    // only when it matches *nothing in the whole target directory*. A scoped
    // status (`dfm status <paths>`) must not judge patterns against just the
    // requested paths — a pattern aimed at a file outside the request would
    // falsely appear unused. So scoped reports skip the analysis entirely
    // (empty `stale_patterns` → no block in the default report), while the
    // explicit `--unused-patterns` flag always walks the whole target dir to
    // give a correct, global answer regardless of any requested scope.
    let scoped = paths.is_some();
    if !scoped || *unused_patterns {
        // The full non-scoped status already walked the whole target dir in
        // Phase 2, so its `traversed_target` and pruned dirs can be reused.
        // Only a scoped `--unused-patterns` needs a fresh full-tree walk.
        let (unused_walk, unused_pruned): (Vec<PathBuf>, Vec<String>) = if scoped {
            let ListDirectories { found, errors, pruned } = list_directory(
                std::slice::from_ref(&target_dir_abs),
                &target_dir_abs,
                Some(TraversalFilter::PruneIgnoredDirs(&target_ignore_regex)),
            )?;
            if !errors.is_empty() {
                return Err(DfmError::InvalidData(format!(
                    "failed to process some subdirectories or files in target directory for status: {:?}",
                    errors
                )));
            }
            (found, pruned)
        } else {
            (traversed_target.clone(), pruned_dirs.clone())
        };

        // A pattern that pruned a directory counts as in use (its `!! dir/`
        // entry is exactly what makes it used in the full report).
        let mut all_relative_paths: Vec<String> = Vec::new();
        for abs in &unused_walk {
            if abs.to_str().is_some() {
                let rel = file_path_relative_to(abs, &target_dir_abs);
                if let Some(rs) = rel.to_str() {
                    all_relative_paths.push(rs.to_string());
                }
            }
        }
        for pruned_rel in &unused_pruned {
            all_relative_paths.push(format!("{}/", pruned_rel));
        }
        // Add all target paths from state entries (already relative)
        for entry in &entries {
            if entry.code != StatusCode::Unpulled {
                all_relative_paths.push(entry.path.clone());
            }
        }

        for pattern_str in target_ignore_regex.patterns() {
            let mut matched_any = false;
            for rel_path in &all_relative_paths {
                if pattern_matches_path_components(pattern_str, rel_path) {
                    matched_any = true;
                    break;
                }
            }
            if !matched_any {
                stale_patterns.push(pattern_str.to_string());
            }
        }
    }

    // ------------------------------------------------------------------
    // Special mode: only list patterns
    // ------------------------------------------------------------------
    if *ignored_patterns {
        let mut out = String::new();
        for p in target_ignore_regex.patterns() {
            out.push_str(p);
            out.push('\n');
        }
        return write_stdout(&out);
    }

    if *unused_patterns {
        if stale_patterns.is_empty() {
            info!("unused ignore patterns");
        } else {
            // Same block shape as the report's stale-patterns section
            let mut out = String::new();
            out.push_str("Unused ignore patterns:\n");
            for p in &stale_patterns {
                out.push_str(&format!("  {}  {}\n", StatusCode::StalePattern, p));
            }
            return write_stdout(&out);
        }
        return Ok(());
    }

    // ------------------------------------------------------------------
    // Apply filters
    // ------------------------------------------------------------------
    // Sort once here so every output mode (porcelain, short, default) is
    // deterministic. Phase 1 iterates a HashMap and Phase 2 a walk, so without
    // this the line order could change between runs.
    entries.sort_by_key(|a| (a.path.clone(), a.code.to_string()));

    let filtered: Vec<&StatusEntry> = entries.iter().filter(|e| {
        if *conflicted && e.code != StatusCode::BothModified { return false; }
        if *modified && !e.code.is_modified() { return false; }
        if *unmanaged && e.code != StatusCode::Unmanaged && e.code != StatusCode::UnmanagedSymlink { return false; }
        if *managed && !e.code.is_managed() { return false; }
        if *unpulled && e.code != StatusCode::Unpulled { return false; }
        if *ignored && !e.code.is_ignored() { return false; }
        if !*all && !*ignored && e.code.is_ignored() { return false; }
        if !*all && !*managed && e.code.is_up_to_date() { return false; }
        true
    }).collect();

    // ------------------------------------------------------------------
    // Output
    // ------------------------------------------------------------------
    let git_info = get_git_info(&source_dir_abs);

    if *porcelain {
        // Tab-separated, stable, never paged
        let mut out = String::new();
        for entry in &filtered {
            out.push_str(&format!("{}\t{}\n", entry.code, entry.path));
        }
        if filtered.is_empty() {
            // If we have stale patterns, output them too
            for p in &stale_patterns {
                out.push_str(&format!("{}\t{}\n", StatusCode::StalePattern, p));
            }
        }
        write_stdout(&out)
    } else if *short {
        let mut out = String::new();
        for entry in &filtered {
            out.push_str(&format!("{} {}\n", entry.code, entry.path));
        }
        write_stdout(&out)
    } else {
        let has_managed = entries.iter().any(|e| e.code.is_managed());
        let output = format_default(&filtered, &entries, &stale_patterns, git_info.as_deref(), &target_dir_abs, &source_dir_abs, has_managed);
        print_paged(&output)?;
        Ok(())
    }
}

// Phase 2 classifiers (target directory walk)

/// A target path is a symlink. Classify it as a managed symlink (`LL`, only
/// shown with `--all`), an ignored symlink (`!L`), or an unmanaged symlink
/// (`?L`). A symlink counts as managed when either its pointer file is in
/// state or its resolved pointee maps to a managed source copy.
#[allow(clippy::too_many_arguments)]
fn classify_target_symlink(
    settings: &Settings,
    target_dir_abs: &Path,
    source_dir_abs: &Path,
    target_ignore_regex: &RegexSet,
    target_abs: &PathBuf,
    rel_str: &str,
    state_keys: &HashSet<String>,
    all: bool,
    ignored: bool,
    entries: &mut Vec<StatusEntry>,
) {
    // Managed via a source symlink pointer file in state.
    let pointer_path = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs, source_dir_abs,
        target_abs, Some(&settings.symlink_postfix),
    );
    let pointer_rel = file_path_relative_to(&pointer_path, source_dir_abs);
    let pointer_rel = remove_dots_from_path(&pointer_rel);
    let pointer_in_state = state_keys.contains(pointer_rel.to_str().unwrap_or(""));

    // Or via a managed pointee that resolves into the source directory — the
    // `--symlink` pattern. A pointee inside the target dir or elsewhere does
    // NOT make the symlink managed: if it is not in state.syncs it should
    // appear as `?L` so the user can decide to add it.
    let pointee_in_state = fs::read_link(target_abs)
        .ok()
        .and_then(|link_target| {
            let abs = target_abs.parent().unwrap_or(std::path::Path::new(".")).join(&link_target);
            fs::canonicalize(&abs).ok()
        })
        .map(|pointee_abs| {
            if pointee_abs.starts_with(source_dir_abs) {
                let rel_str = state_key_for(&pointee_abs, source_dir_abs);
                state_keys.contains(rel_str.as_str())
            } else {
                false
            }
        })
        .unwrap_or(false);

    if pointer_in_state {
        // Phase 1 already emitted the `LL` entry for this symlink's
        // `*.symlink` state key (it iterates every state entry). Returning
        // here keeps `--all` output free of duplicate lines.
        return;
    }

    if pointee_in_state {
        if all {
            entries.push(StatusEntry {
                code: StatusCode::ManagedSymlink,
                path: rel_str.to_string(),
                matched_pattern: None,
            });
        }
        return;
    }

    if let Some(pattern) = check_path_matches_regex_component_wise(target_ignore_regex, &PathBuf::from(rel_str)) {
        if all || ignored {
            entries.push(StatusEntry {
                code: StatusCode::IgnoredSymlink,
                path: rel_str.to_string(),
                matched_pattern: Some(pattern),
            });
        }
        return;
    }

    entries.push(StatusEntry {
        code: StatusCode::UnmanagedSymlink,
        path: rel_str.to_string(),
        matched_pattern: None,
    });
}

/// A target path is a regular file. Classify it as already-managed (skip),
/// ignored (`!!`), or unmanaged (`??`).
#[allow(clippy::too_many_arguments)]
fn classify_target_file(
    settings: &Settings,
    target_dir_abs: &Path,
    source_dir_abs: &Path,
    target_ignore_regex: &RegexSet,
    target_abs: &Path,
    rel_str: &str,
    state_keys: &HashSet<String>,
    all: bool,
    ignored: bool,
    entries: &mut Vec<StatusEntry>,
) {
    // Already in state (plain, encrypted, or symlink variant) — covered by Phase 1.
    let source_abs = filepath_in_source_dir(
        &settings.dot_prefix, target_dir_abs, source_dir_abs,
        target_abs, None,
    );
    let source_rel_str = state_key_for(&source_abs, source_dir_abs);

    let enc_key = format!("{}{}", source_rel_str, settings.encrypted_postfix);
    let sym_key = format!("{}{}", source_rel_str, settings.symlink_postfix);

    if state_keys.contains(&source_rel_str)
        || state_keys.contains(&enc_key)
        || state_keys.contains(&sym_key)
    {
        return;
    }

    if let Some(pattern) = check_path_matches_regex_component_wise(target_ignore_regex, &PathBuf::from(rel_str)) {
        if all || ignored {
            entries.push(StatusEntry {
                code: StatusCode::Ignored,
                path: rel_str.to_string(),
                matched_pattern: Some(pattern),
            });
        }
        return;
    }

    entries.push(StatusEntry {
        code: StatusCode::Unmanaged,
        path: rel_str.to_string(),
        matched_pattern: None,
    });
}

// Default categorized output

/// Collapse a set of paths (code, path, matched-pattern) so that a directory
/// with ≥2 entries beneath it is shown as a single `{dir}/*` entry.
///
/// `blocked` holds every path that is *not* part of the group being folded
/// (files ignored, up-to-date, tracked-but-unlisted, another status group,
/// or an ignored pruned dir). A directory is only foldable when nothing in
/// `blocked` lies beneath it — folding `dir/*` would otherwise hide a file
/// the user needs to see separately. So `.config/dir1/file` + `.config/dir3/file`
/// with an ignored `.config/dir2` still prints both files individually.
///
/// Iteration is deepest-first: each pass picks the deepest ancestor directory
/// that has ≥2 descendants and no blocked path beneath it, replaces everything
/// under it with one `{dir}/*`, and repeats so `a/b/x + a/b/y` becomes
/// `a/b/*` and then propagates to `a/*`. Paths already marked `*` are never
/// collapsed further. The `matched_pattern` is dropped from a collapsed entry.
fn collapse_shared_dirs(
    paths: &[(StatusCode, String, Option<String>)],
    blocked: &BTreeSet<String>,
) -> Vec<(StatusCode, String, Option<String>)> {
    let mut paths = paths.to_vec();
    loop {
        let mut ancestor_counts: BTreeMap<String, usize> = BTreeMap::new();
        for (_, path, _) in &paths {
            let parts: Vec<&str> = path.split('/').collect();
            let mut prefix = String::new();
            for (i, part) in parts.iter().enumerate() {
                if *part == "*" {
                    break; // don't collapse through a wildcard marker
                }
                if i > 0 {
                    prefix.push('/');
                }
                prefix.push_str(part);
                if i + 1 < parts.len() {
                    *ancestor_counts.entry(prefix.clone()).or_default() += 1;
                }
            }
        }

        // Find the deepest ancestor with ≥2 entries beneath it that is not
        // severed by a blocked path. Fold only full-coverage gaps.
        let Some(collapsed_dir) = ancestor_counts
            .into_iter()
            .filter(|(prefix, count)| {
                *count >= 2 && !is_blocked(prefix, blocked)
            })
            .max_by_key(|(prefix, _)| prefix.matches('/').count())
            .map(|(prefix, _)| prefix)
        else {
            break;
        };

        // Replace every entry under `collapsed_dir` with a single
        // `collapsed_dir/*` entry.
        let mut next = Vec::new();
        let mut collapsed_added = false;
        let dir_prefix = format!("{}/", collapsed_dir);
        for (code, path, pattern) in &paths {
            if path == &collapsed_dir || path.starts_with(&dir_prefix) {
                if !collapsed_added {
                    next.push((*code, format!("{}/*", collapsed_dir), None));
                    collapsed_added = true;
                }
                continue;
            }
            next.push((*code, path.clone(), pattern.clone()));
        }
        paths = next;
    }
    paths
}

/// A directory `dir` is blocked (not foldable) when some non-group path is
/// exactly `dir` or starts with `dir/`. Only the deepest still-open ancestor
/// is relevant, but testing each prefix against the whole blocked set is simpler
/// and correct because a blocked path always severs every ancestor above it.
fn is_blocked(dir: &str, blocked: &BTreeSet<String>) -> bool {
    let dir_prefix = format!("{}/", dir);
    // The empty string is never a real collapsed dir; the root must not be
    // sewn together with anything else.
    if dir.is_empty() {
        return false;
    }
    blocked
        .iter()
        .any(|b| b == dir || b.starts_with(&dir_prefix))
}

/// Color a path by its status code. Only used in the human-readable default
/// output; porcelain/short paths stay raw so their output is deterministic.
fn color_path(code: StatusCode, path: &str) -> String {
    match code {
        StatusCode::BothModified => path.red().to_string(),
        StatusCode::TargetModified | StatusCode::SourceModified => path.yellow().to_string(),
        StatusCode::UpToDate => path.green().to_string(),
        _ => path.to_string(),
    }
}

fn format_default(entries: &[&StatusEntry], all_entries: &[StatusEntry], stale_patterns: &[String], git_info: Option<&str>, target_dir_abs: &Path, source_dir_abs: &Path, has_managed: bool) -> String {
    let mut out = String::new();

    // Header — replace home directory prefix with ~
    let target_str = tilde_path(target_dir_abs.to_str().unwrap_or("~"));
    let source_str = tilde_path(source_dir_abs.to_str().unwrap_or("?"));
    if let Some(ref info) = git_info {
        out.push_str(&format!("Source: {}  ({})\n", source_str, info));
    } else {
        out.push_str(&format!("Source: {}\n", source_str));
    }
    out.push_str(&format!("Target: {}\n", target_str));
    if entries.is_empty() {
        if has_managed {
            out.push_str("All up-to-date.\n");
        } else {
            out.push_str("No files managed.\n");
        }
    } else {
        out.push('\n');
    }

    // Group entries by code
    let mut merge: Vec<&StatusEntry> = Vec::new();
    let mut add: Vec<&StatusEntry> = Vec::new();
    let mut pull: Vec<&StatusEntry> = Vec::new();
    let mut unmanaged: Vec<&StatusEntry> = Vec::new();
    let mut unpulled: Vec<&StatusEntry> = Vec::new();
    let mut ignored: Vec<&StatusEntry> = Vec::new();
    let mut uptodate: Vec<&StatusEntry> = Vec::new();

    for e in entries {
        match e.code {
            StatusCode::BothModified => merge.push(e),
            StatusCode::TargetModified => add.push(e),
            StatusCode::SourceModified => pull.push(e),
            StatusCode::Unmanaged | StatusCode::UnmanagedSymlink => unmanaged.push(e),
            StatusCode::Unpulled => unpulled.push(e),
            StatusCode::Ignored | StatusCode::IgnoredSymlink => ignored.push(e),
            StatusCode::UpToDate | StatusCode::ManagedSymlink | StatusCode::NeverSynchronized => uptodate.push(e),
            StatusCode::StalePattern => {}
        }
    }

    // Helper to write a group
    let write_group = |out: &mut String, header: &str, items: &[&StatusEntry], is_last_group: bool| {
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("{}:\n", header));

        // Fold shared directories: a `dir/*` is only emitted when *every* path
        // under `dir` belongs to this group. Paths of any other status (ignored
        // pruned dirs, up-to-date / tracked files, parallel groups) sever the
        // fold, so an ignored sibling directory keeps files listed individually.
        let member_paths: BTreeSet<&str> = items.iter().map(|i| i.path.as_str()).collect();
        let blocked: BTreeSet<String> = all_entries
            .iter()
            .filter(|e| !member_paths.contains(e.path.as_str()))
            .map(|e| e.path.clone())
            .collect();

        // Build display paths, then collapse shared directories (e.g.
        // multiple files under dir/ to a single dir/* entry).
        let paths = collapse_shared_dirs(&items.iter()
            .map(|item| (item.code, item.path.clone(), item.matched_pattern.clone()))
            .collect::<Vec<_>>(), &blocked);

        // Build the final display list.
        struct DispLine {
            code: StatusCode,
            path: String,
            pattern: Option<String>,
        }
        let display: Vec<DispLine> = paths.into_iter()
            .map(|(code, path, pattern)| DispLine { code, path, pattern })
            .collect();

        // Align parenthesised patterns (matched_pattern) so '(' starts at the same column.
        let max_path_len = display
            .iter()
            .filter_map(|d| d.pattern.as_ref().map(|_| d.path.len()))
            .max()
            .unwrap_or(0);

        for d in &display {
            if let Some(ref pat) = d.pattern {
                out.push_str(&format!("  {}  {:<max_width$}  ({})\n", d.code, color_path(d.code, &d.path), pat, max_width = max_path_len));
            } else {
                out.push_str(&format!("  {}  {}\n", d.code, color_path(d.code, &d.path)));
            }
        }
        // Do not print after the last group in list as 'ls -lR' shell command
        if !is_last_group {
            out.push('\n');
        }
    };

    let group_order = [merge.is_empty(),
        add.is_empty(),
        pull.is_empty(),
        unpulled.is_empty(),
        unmanaged.is_empty(),
        uptodate.is_empty(),
        ignored.is_empty(),
        stale_patterns.is_empty()];

    let mut group_lastness = vec![];
    for i in 0..group_order.len() {
        // If all from i to the right are empty
        // then ith group is the last to be printed
        let is_last = group_order.iter()
            .skip(i + 1)
            .all(|&se| se);
        group_lastness.push(is_last);
    }

    write_group(&mut out, "Changes to merge", &merge, group_lastness[0]);
    write_group(&mut out, "Changes to add", &add, group_lastness[1]);
    write_group(&mut out, "Changes to pull", &pull, group_lastness[2]);
    write_group(&mut out, "Unpulled", &unpulled, group_lastness[3]);
    write_group(&mut out, "Unmanaged files", &unmanaged, group_lastness[4]);
    write_group(&mut out, "Up to date", &uptodate, group_lastness[5]);
    write_group(&mut out, "Ignored", &ignored, group_lastness[6]);

    // Stale patterns
    if !stale_patterns.is_empty() {
        out.push_str("Unused ignore patterns:\n");
        for p in stale_patterns {
            out.push_str(&format!("  {}  {}\n", StatusCode::StalePattern, p));
        }
    }

    out
}

// Pager

/// Write a string to stdout, tolerating a broken pipe: when the reader closes
/// the stream on purpose (e.g. `dfm status | head -1`) the write-failed error
/// from the closed pipe is not a failure of this command.
fn write_stdout(s: &str) -> Result<(), DfmError> {
    use std::io::Write;
    let mut out = std::io::stdout();
    match out.write_all(s.as_bytes()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(DfmError::Io(e)),
    }
}

fn print_paged(output: &str) -> Result<(), DfmError> {
    // The pager only makes sense for an interactive terminal: when stdout is a
    // pipe or file, page the output directly (a pipe cannot be scrolled, and a
    // captured pager would corrupt e.g. `dfm status | grep`).
    if !std::io::stdout().is_terminal() {
        return write_stdout(output);
    }
    // Detect terminal height
    let line_count = output.lines().count();
    let term_height = terminal_height().unwrap_or(24);

    if line_count > term_height {
        // Use pager
        // TODO create a field in config file and settings for the paging command
        // by default in config it must be 'less -FRSX', but env PAGER has the priority.
        let pager_cmd = env::var("PAGER").unwrap_or_else(|_| "less -FRSX".to_string());
        let (prog, args) = split_command(&pager_cmd);
        let Some(prog) = prog else {
            return write_stdout(output);
        };
        let mut child = ProcessCmd::new(prog)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            // The pager may exit early (e.g. `less` quit by the user): the
            // resulting broken pipe is expected, not an error.
            if let Err(e) = stdin.write_all(output.as_bytes())
                && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                return Err(DfmError::Io(e));
            }
            if let Err(e) = stdin.flush()
                && e.kind() != std::io::ErrorKind::BrokenPipe
            {
                return Err(DfmError::Io(e));
            }
        }
        child.wait()?;
        Ok(())
    } else {
        write_stdout(output)
    }
}

fn terminal_height() -> Option<usize> {
    // Try via `stty size`
    if let Ok(output) = ProcessCmd::new("stty").arg("size").stdout(Stdio::piped()).stderr(Stdio::null()).output()
        && let Ok(s) = String::from_utf8(output.stdout)
        && let Some(rows_str) = s.split_whitespace().next()
        && let Ok(rows) = rows_str.parse::<usize>()
    {
        return Some(rows);
    }
    None
}

// Git integration

fn get_git_info(source_dir: &Path) -> Option<String> {
    let source_dir_str = source_dir.to_string_lossy();
    // Single call, not two: `--branch` emits a `## <branch>...<upstream>
    // [ahead N, behind M]` header line plus one line per uncommitted change, so
    // the branch name, the dirty/clean state and the ahead/behind delta all
    // come from the porcelain output.
    let output = ProcessCmd::new("git")
        .args(["-C", source_dir_str.as_ref(), "status", "--porcelain", "-b"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    let mut lines = text.lines();
    // First line is always the branch header, e.g. `## main...origin/main [behind 2]`.
    let header = lines.next()?.trim().trim_start_matches("## ");

    // Branch name = first token, minus any `...upstream` suffix. Detached
    // (`HEAD (no branch)`) and unborn (`No commits yet on …`) heads have no
    // branch to report — nothing useful to show.
    let branch = header.split_whitespace().next()?.split("...").next()?;
    if branch == "HEAD" || header.contains("No commits yet") || branch.is_empty() {
        return None;
    }

    // dirty = any remaining non-empty line beyond the header, i.e. uncommitted
    // working-tree changes (ahead/behind commits do not count as dirty).
    let dirty = lines.any(|l| !l.trim().is_empty());

    // ahead/behind delta from the `[ … ]` section of the header, if present.
    let mut parts: Vec<String> = Vec::new();
    if let Some(bracket) = header
        .find('[')
        .and_then(|i| header[i + 1..].split(']').next())
    {
        for item in bracket.split(',').map(str::trim) {
            if item.starts_with("ahead ") || item.starts_with("behind ") {
                parts.push(item.to_string());
            }
        }
    }
    parts.push(if dirty { "dirty" } else { "clean" }.to_string());

    Some(format!("branch: {}, {}", branch, parts.join(", ")))
}

// Helpers

/// Replace the home directory prefix with "~" for display.
fn tilde_path(path: &str) -> String {
    if let Ok(home) = env::var("HOME") {
        if path == home {
            return "~".to_string();
        }
        if path.starts_with(&home) {
            // Path starts with home, e.g. /home/user/foo → ~/foo
            if home.len() < path.len() {
                return format!("~{}", &path[home.len()..]);
            }
        }
    }
    path.to_string()
}
