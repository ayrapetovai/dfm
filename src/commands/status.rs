use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCmd, Stdio};

use colored::Colorize;
use log::{debug, info};
use regex::RegexSet;

use super::{
    cli_path_in_scope, list_directory_with_progress, matches_source_ignore_regex, print_paged,
    source_rel_to_target_abs, state_key_for, write_stdout,
};
use crate::DfmError;
use dfm::*;
use microxdg::Xdg;

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
    pub encrypted: bool,
    pub ignored: bool,
    pub ignored_patterns: bool,
    pub unused_patterns: bool,
    /// Restrict the report to these paths (absolute or relative to the current
    /// working directory; each must lie under the target or source directory).
    /// `None` shows the full report over the whole target dir.
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
        matches!(
            self,
            StatusCode::BothModified | StatusCode::TargetModified | StatusCode::SourceModified
        )
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
    /// Part of the encrypted set: managed via a `.encrypted` source file.
    encrypted: bool,
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
                let abs = cli_path_in_scope(p, target_dir_abs, source_dir_abs)?;
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

pub fn status_command(
    settings: &Settings,
    xdg: &Xdg,
    args: StatusArgs,
    state: &StateObject,
) -> Result<(), DfmError> {
    let StatusArgs {
        ref all,
        ref short,
        ref porcelain,
        ref conflicted,
        ref modified,
        ref unmanaged,
        ref managed,
        ref unpulled,
        ref encrypted,
        ref ignored,
        ref ignored_patterns,
        ref unused_patterns,
        ref paths,
    } = args;

    let (target_dir_abs, source_dir_abs) = calc_working_dir_paths(settings)?;

    let target_ignore_file = calc_local_ignore_file(xdg)?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file)?;

    // Source-side ignore patterns (.dfm_ignore_file): a state entry whose
    // source path matches must not be offered as pullable — the file is
    // non-syncable content of the source directory itself.
    let source_ignore_file = calc_source_ignore_file(&source_dir_abs);
    let source_ignore_regex = load_ignore_regex(&source_ignore_file)?;

    // Restrict the report to the requested paths (absolute or relative to the
    // target directory). With no paths, the whole target dir is analyzed.
    let requested_roots =
        resolve_status_paths(paths.as_ref(), &target_dir_abs, &source_dir_abs, settings)?;

    // Paths to dfm's own internal files (skip in unmanaged detection). The
    // config file is NOT internal here: it is ordinary user data, managed and
    // reported like any other dotfile.
    let state_file_path = calc_state_file_path(xdg).ok();

    // Phase 1 — Process every state entry (managed files)
    let mut entries: Vec<StatusEntry> = Vec::new();
    let mut state_keys: HashSet<String> = HashSet::new();

    let mut progress = ActionBar::new("reading");
    if *porcelain {
        // `--porcelain` output keeps stdout byte-clean even on a terminal.
        progress = ActionBar::suppressed("reading");
    }
    for (i, (source_rel, sync_time)) in state.syncs.iter().enumerate() {
        progress.set(i + 1, Some(state.syncs.len()));
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
        if !requested_roots
            .iter()
            .any(|root| target_abs.starts_with(root))
        {
            continue;
        }

        debug!(
            "status: state entry {:?} → target {:?}",
            source_rel, target_abs
        );

        // Check if this is a managed symlink (state key ends with symlink_postfix)
        let is_managed_symlink = source_rel.ends_with(&settings.symlink_postfix);
        // Part of the encrypted set: managed via a `.encrypted` source file.
        let is_encrypted = source_rel.ends_with(&settings.encrypted_postfix);

        // Check ignore patterns
        if let Some(pattern) = check_path_matches_regex_component_wise(
            &target_ignore_regex,
            &PathBuf::from(&target_rel),
        ) {
            let code = if is_managed_symlink {
                StatusCode::IgnoredSymlink
            } else {
                StatusCode::Ignored
            };
            entries.push(StatusEntry {
                code,
                path: target_rel.clone(),
                matched_pattern: Some(pattern),
                encrypted: is_encrypted,
            });
            continue;
        }

        // Source-side patterns classify the same way: the entry stays out of
        // the pullable groups (a removed target copy is not "Unpulled").
        if let Some(pattern) =
            matches_source_ignore_regex(&source_ignore_regex, source_rel, settings)
        {
            let code = if is_managed_symlink {
                StatusCode::IgnoredSymlink
            } else {
                StatusCode::Ignored
            };
            entries.push(StatusEntry {
                code,
                path: target_rel.clone(),
                matched_pattern: Some(pattern),
                encrypted: is_encrypted,
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
                debug!(
                    "status: stale state entry {:?}, source symlink missing",
                    source_rel
                );
                continue;
            }
            let code = if target_exists {
                StatusCode::ManagedSymlink
            } else {
                StatusCode::Unpulled
            };
            entries.push(StatusEntry {
                code,
                path: target_rel.clone(),
                matched_pattern: None,
                encrypted: is_encrypted,
            });
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
            let cmp = match compare_files(
                &settings.encrypted_postfix,
                &target_abs,
                &source_abs,
                Some(sync_time),
            ) {
                Ok(cmp) => cmp,
                Err(e) if e.is_permission_denied() => {
                    warn_unreadable(&target_abs, &e);
                    continue;
                }
                Err(e) => return Err(e),
            };
            match cmp {
                CompareByTimestamp::BothModified => (StatusCode::BothModified, target_rel.clone()),
                CompareByTimestamp::TargetModified => {
                    (StatusCode::TargetModified, target_rel.clone())
                }
                CompareByTimestamp::SourceModified => {
                    (StatusCode::SourceModified, target_rel.clone())
                }
                CompareByTimestamp::NonModified => (StatusCode::UpToDate, target_rel.clone()),
                CompareByTimestamp::NeverSynchronized => {
                    (StatusCode::NeverSynchronized, target_rel.clone())
                }
            }
        } else {
            state_keys.remove(source_rel);
            debug!(
                "status: stale state entry {:?}, both sides missing",
                source_rel
            );
            continue;
        };

        entries.push(StatusEntry {
            code,
            path,
            matched_pattern: None,
            encrypted: is_encrypted,
        });
    }
    progress.clear();

    // Phase 2 — Walk target directory for unmanaged files
    let ListDirectories {
        found: traversed_target,
        errors: traversal_errors,
        pruned: pruned_dirs,
    } = list_directory_with_progress(
        &requested_roots,
        &target_dir_abs,
        Some(TraversalFilter::PruneIgnoredDirs(&target_ignore_regex)),
        &mut |visited| progress.set(visited, None),
    )?;
    if !traversal_errors.is_empty() {
        return Err(DfmError::InvalidData(format!(
            "failed to process some subdirectories or files in target directory for status: {:?}",
            traversal_errors
        )));
    }

    // Phase 3 builds its own list from traversed_target + pruned dirs + entries

    // Pre-compute canonical source dir for robust path comparison
    let canon_source_dir =
        fs::canonicalize(&source_dir_abs).unwrap_or_else(|_| source_dir_abs.clone());

    for (i, target_abs) in traversed_target.iter().enumerate() {
        progress.set(i + 1, Some(traversed_target.len()));
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

        // Skip known dfm internal files (state, ignore)
        if let Some(ref sfp) = state_file_path
            && *target_abs == *sfp
        {
            continue;
        }
        if *target_abs == target_ignore_file {
            continue;
        }

        // Compute the relative path for the display
        let rel_str = state_key_for(target_abs, &target_dir_abs);

        // An explicitly requested scope unhides the Ignored category: naming
        // a path means "tell me about this scope", so the flood-prevention
        // reason for hiding ignored entries does not apply.
        let show_ignored = *all || *ignored || paths.is_some();

        if target_abs.is_symlink() {
            classify_target_symlink(
                settings,
                &target_dir_abs,
                &source_dir_abs,
                &target_ignore_regex,
                target_abs,
                &rel_str,
                &state_keys,
                *all,
                show_ignored,
                &mut entries,
            );
        } else {
            classify_target_file(
                settings,
                &target_dir_abs,
                &source_dir_abs,
                &target_ignore_regex,
                target_abs,
                &rel_str,
                &state_keys,
                show_ignored,
                &mut entries,
            );
        }
    }

    // Entries for fully-ignored directories that were pruned during the walk:
    // one `!! dir/` per directory instead of enumerating every file inside it.
    for pruned_rel in &pruned_dirs {
        let matched_pattern = dir_ignore_pattern(&target_ignore_regex, pruned_rel);
        entries.push(StatusEntry {
            code: StatusCode::Ignored,
            path: format!("{}/", pruned_rel),
            matched_pattern,
            encrypted: false,
        });
    }

    // Phase 3 — Find unused ignore patterns
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
            let ListDirectories {
                found,
                errors,
                pruned,
            } = list_directory_with_progress(
                std::slice::from_ref(&target_dir_abs),
                &target_dir_abs,
                Some(TraversalFilter::PruneIgnoredDirs(&target_ignore_regex)),
                &mut |visited| progress.set(visited, None),
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
        for (i, abs) in unused_walk.iter().enumerate() {
            progress.set(i + 1, Some(unused_walk.len()));
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

        // Mark which patterns still match something. The old nested loop ran
        // every pattern against every path (O(patterns × paths)). Instead,
        // probe each path through the RegexSet once: `matches` returns every
        // pattern whose raw regex hits as a *substring*, then verify each hit
        // with the exact component-wise matcher and mark the pattern used.
        //
        // For anchor-free patterns the substring hit is a superset of the
        // component-wise truth (a component fully matching a sub-pattern is a
        // substring match at the same position), so the prefilter can only
        // over-approximate — never miss a real use — and the verify step keeps
        // the result exact. Patterns containing `^`/`$` break that implication
        // (e.g. `^my\.log$` matches any component but only a string-start
        // substring), so those resolve with the exhaustive per-path check.
        let patterns = target_ignore_regex.patterns();
        let anchored: HashSet<usize> = patterns
            .iter()
            .enumerate()
            .filter(|(_, p)| p.contains('^') || p.contains('$'))
            .map(|(i, _)| i)
            .collect();
        let mut used = vec![false; patterns.len()];

        for (i, rel_path) in all_relative_paths.iter().enumerate() {
            progress.set(i + 1, Some(all_relative_paths.len()));
            for idx in target_ignore_regex.matches(rel_path).iter() {
                if !anchored.contains(&idx)
                    && pattern_matches_path_components(&patterns[idx], rel_path)
                {
                    used[idx] = true;
                }
            }
        }
        for (i, idx) in anchored.iter().enumerate() {
            progress.set(i + 1, Some(anchored.len()));
            let pattern = &patterns[*idx];
            if all_relative_paths
                .iter()
                .any(|rel_path| pattern_matches_path_components(pattern, rel_path))
            {
                used[*idx] = true;
            }
        }

        stale_patterns = patterns
            .iter()
            .zip(used.iter())
            .filter(|(_, used)| !**used)
            .map(|(p, _)| p.to_string())
            .collect();
    }

    // Special mode: only list patterns
    if *ignored_patterns {
        let mut out = String::new();
        for p in target_ignore_regex.patterns() {
            out.push_str(p);
            out.push('\n');
        }
        progress.clear();
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
            progress.clear();
            return write_stdout(&out);
        }
        return Ok(());
    }

    // Apply filters
    // Sort once here so every output mode (porcelain, short, default) is
    // deterministic. Phase 1 iterates a HashMap and Phase 2 a walk, so without
    // this the line order could change between runs.
    entries.sort_by_key(|a| (a.path.clone(), a.code.to_string()));

    // An explicitly requested scope unhides the Ignored category: naming a
    // path means "tell me about this scope", so the flood-prevention reason
    // for hiding ignored entries does not apply. --all/--ignored keep their
    // meaning for the unscoped report; other flags keep priority.
    let explicit_paths = paths.is_some();

    let filtered: Vec<&StatusEntry> = entries
        .iter()
        .filter(|e| {
            // `-e` overrides all other filters: report only the encrypted set, in
            // whatever status category each entry belongs to.
            if *encrypted {
                return e.encrypted;
            }
            // Base visibility, independent of which filter flags are set: ignored
            // entries stay hidden unless `--all`, `--ignored`, or an explicit
            // scope asks for them; up-to-date entries stay hidden unless `--all`
            // or `--managed` asks for them.
            if !*all && !*ignored && !explicit_paths && e.code.is_ignored() {
                return false;
            }
            if !*all && !*managed && e.code.is_up_to_date() {
                return false;
            }

            // With no filter flag the default report shows every visible entry.
            // Combining filter flags is additive: each enabled flag contributes
            // its own set and the report shows the union — `--managed --unmanaged`
            // shows both blocks — with no priority between the flags.
            let any_category_filter =
                *conflicted || *modified || *unmanaged || *managed || *unpulled || *ignored;
            if !any_category_filter {
                return true;
            }
            (*conflicted && e.code == StatusCode::BothModified)
                || (*modified && e.code.is_modified())
                || (*unmanaged
                    && (e.code == StatusCode::Unmanaged || e.code == StatusCode::UnmanagedSymlink))
                || (*managed && e.code.is_managed())
                || (*unpulled && e.code == StatusCode::Unpulled)
                || (*ignored && e.code.is_ignored())
        })
        .collect();

    // Output
    // Show 100 % to cover the sort/filter phase before clearing the bar.
    if !entries.is_empty() {
        progress.set(entries.len(), Some(entries.len()));
    }
    progress.clear();
    let git_info = get_git_info(&source_dir_abs);

    // A restrictive filter asks for one specific list, so the unused-patterns
    // block belongs only to the unfiltered report (and to the dedicated
    // --unused-patterns mode). --all only unhides categories — it keeps the
    // block; scoped PATHS already suppress it via empty `stale_patterns`.
    let restrictive_filter =
        *conflicted || *modified || *unmanaged || *managed || *unpulled || *encrypted || *ignored;
    let report_stale: &[String] = if restrictive_filter {
        &[]
    } else {
        &stale_patterns
    };

    if *porcelain {
        // Tab-separated, stable, never paged
        let mut out = String::new();
        for entry in &filtered {
            out.push_str(&format!("{}\t{}\n", entry.code, entry.path));
        }
        if filtered.is_empty() {
            // If we have stale patterns, output them too
            for p in report_stale {
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
        // Show the Unpulled block when explicitly requested (`--unpulled`) or
        // when `--all` unhides every category.
        let show_unpulled = *unpulled || *all;
        let mut formatting = ActionBar::new("formatting output");
        let output = format_default(
            &filtered,
            &entries,
            report_stale,
            git_info.as_deref(),
            &target_dir_abs,
            &source_dir_abs,
            has_managed,
            show_unpulled,
            &mut formatting,
        );
        formatting.clear();
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
    show_ignored: bool,
    entries: &mut Vec<StatusEntry>,
) {
    // Managed via a source symlink pointer file in state.
    let pointer_path = filepath_in_source_dir(
        &settings.dot_prefix,
        target_dir_abs,
        source_dir_abs,
        target_abs,
        Some(&settings.symlink_postfix),
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
            let abs = target_abs
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join(&link_target);
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
                encrypted: false,
            });
        }
        return;
    }

    if let Some(pattern) =
        check_path_matches_regex_component_wise(target_ignore_regex, &PathBuf::from(rel_str))
    {
        if show_ignored {
            entries.push(StatusEntry {
                code: StatusCode::IgnoredSymlink,
                path: rel_str.to_string(),
                matched_pattern: Some(pattern),
                encrypted: false,
            });
        }
        return;
    }

    entries.push(StatusEntry {
        code: StatusCode::UnmanagedSymlink,
        path: rel_str.to_string(),
        matched_pattern: None,
        encrypted: false,
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
    show_ignored: bool,
    entries: &mut Vec<StatusEntry>,
) {
    // Already in state (plain, encrypted, or symlink variant) — covered by Phase 1.
    let source_abs = filepath_in_source_dir(
        &settings.dot_prefix,
        target_dir_abs,
        source_dir_abs,
        target_abs,
        None,
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

    if let Some(pattern) =
        check_path_matches_regex_component_wise(target_ignore_regex, &PathBuf::from(rel_str))
    {
        if show_ignored {
            entries.push(StatusEntry {
                code: StatusCode::Ignored,
                path: rel_str.to_string(),
                matched_pattern: Some(pattern),
                encrypted: false,
            });
        }
        return;
    }

    entries.push(StatusEntry {
        code: StatusCode::Unmanaged,
        path: rel_str.to_string(),
        matched_pattern: None,
        encrypted: false,
    });
}

// Default categorized output

/// Collapse a set of paths (code, path, matched-pattern, encrypted) so that a
/// directory with ≥2 entries beneath it is shown as a single `{dir}/*` entry.
///
/// `blocked` holds every path that is *not* part of the group being folded
/// (files ignored, up-to-date, tracked-but-unlisted, another status group,
/// or an ignored pruned dir). A directory is only foldable when nothing in
/// `blocked` lies beneath it — folding `dir/*` would otherwise hide a file
/// the user needs to see separately. So `.config/dir1/file` + `.config/dir3/file`
/// with an ignored `.config/dir2` still prints both files individually.
///
/// Foldability is decided bottom-up in a single pass (O(n × depth), no rescans):
/// counting every blocked path's ancestor prefixes once, tallying the group's
/// descendants under each prefix, then folding prefixes deepest-first. A dir
/// folds only when, after its directly-folding children collapse to one member
/// each, at least two members remain beneath it — so `a/b/x + a/b/y` becomes
/// `a/b/*` and `a/c` converges with it into `a/*`, while a chain empty except
/// for a collapsing leaf (`a/b/c x2`) folds only at `a/b/c/*`. Each final
/// folded directory is emitted as one `{dir}/*` entry at the position of
/// its first member. Paths already marked `*` are never crossed. The
/// `matched_pattern` is dropped from a collapsed entry; the collapsed entry's
/// `encrypted` flag is true only when every member of the group is encrypted
/// (so `dir/* (encrypted)` is emitted only for a wholly encrypted directory).
///
/// Every path examined advances `formatting` by one (`examined` accumulates
/// across callers so the counter stays monotonic through all report groups).
fn collapse_shared_dirs(
    paths: &[(StatusCode, String, Option<String>, bool)],
    blocked: &BTreeSet<String>,
    formatting: &mut ActionBar,
    examined: &mut usize,
) -> Vec<(StatusCode, String, Option<String>, bool)> {
    // A blocked path severs every ancestor directory above it too, so compute
    // the closure of blocked prefixes once: O(|blocked| × depth) for setup,
    // then each membership test below is O(1).
    let mut blocked_prefixes: BTreeSet<String> = BTreeSet::new();
    for blocked_path in blocked {
        let parts: Vec<&str> = blocked_path.split('/').collect();
        let mut prefix = String::new();
        for (i, part) in parts.iter().enumerate() {
            if *part == "*" {
                break; // never fold through a wildcard marker
            }
            if i > 0 {
                prefix.push('/');
            }
            prefix.push_str(part);
            blocked_prefixes.insert(prefix.clone());
        }
    }

    // Tally how many group paths lie strictly beneath every prefix.
    let mut ancestor_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, path, _, _) in paths {
        let parts: Vec<&str> = path.split('/').collect();
        let mut prefix = String::new();
        for (i, part) in parts.iter().enumerate() {
            if *part == "*" {
                break;
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
    *examined += paths.len();
    formatting.set(*examined, None);

    // Without a prefix that has ≥2 members there is nothing to fold.
    if ancestor_counts.values().all(|&count| count < 2) {
        return paths.to_vec();
    }

    // Decide foldability bottom-up over the candidate-dir tree, mirroring the
    // old deepest-first greedy exactly: a dir's member count is its directly
    // attached leaves plus one for each directly-folding child (a folded child
    // subtree collapses to a single member) plus the member count of every
    // non-folding child. A dir folds only when that count stays ≥ 2 — so a
    // chain `a/b/c` empty except for folding `c` leaves both `a/b` and `a`
    // with one member each, and only `a/b/c` collapses.
    let candidates: BTreeSet<&String> = ancestor_counts.keys().collect();

    let mut children: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
    for candidate in &candidates {
        if let Some(parent) = deepest_candidate_ancestor(candidate, &ancestor_counts) {
            children.entry(parent).or_default().push(candidate);
        }
    }

    // Attach each original path to its deepest candidate ancestor (the path's
    // own parent dir for ordinary entries; the nearest candidate above a `*`
    // component for exotic ones).
    let mut attached_leaves: BTreeMap<&String, usize> = BTreeMap::new();
    for (_, path, _, _) in paths {
        if let Some(attachment) = deepest_candidate_ancestor(path, &ancestor_counts) {
            *attached_leaves.entry(attachment).or_default() += 1;
        }
    }

    let mut order: Vec<&String> = candidates.into_iter().collect();
    order.sort_by_key(|prefix| std::cmp::Reverse(prefix.matches('/').count()));

    let mut member_count: BTreeMap<&String, usize> = BTreeMap::new();
    let mut folds: BTreeSet<String> = BTreeSet::new();
    for dir in order {
        let mut members = attached_leaves.get(dir).copied().unwrap_or(0);
        if let Some(child_dirs) = children.get(dir) {
            for child in child_dirs {
                members += if folds.contains(child.as_str()) {
                    1
                } else {
                    member_count[child]
                };
            }
        }
        member_count.insert(dir, members);
        if members >= 2 && !blocked_prefixes.contains(dir.as_str()) {
            folds.insert(dir.clone());
        }
    }

    // Every member of a folded dir shares the group's code; the folded entry is
    // encrypted only when all of its members are. Record per-fold the position
    // of the first member, so each `dir/*` is emitted exactly once, where its
    // group first appeared in the original order.
    let mut first_member: BTreeMap<&String, usize> = BTreeMap::new();
    let mut member_code: BTreeMap<&String, StatusCode> = BTreeMap::new();
    let mut member_encrypted: BTreeMap<&String, bool> = BTreeMap::new();
    for (index, (code, path, _, encrypted)) in paths.iter().enumerate() {
        if let Some(fold) = outer_fold(path, &folds) {
            first_member.entry(fold).or_insert(index);
            member_code.entry(fold).or_insert(*code);
            *member_encrypted.entry(fold).or_insert(true) &= *encrypted;
        }
    }

    let mut result = Vec::with_capacity(paths.len());
    for (index, (code, path, pattern, encrypted)) in paths.iter().enumerate() {
        if let Some(fold) = outer_fold(path, &folds) {
            if first_member[fold] == index {
                result.push((
                    member_code[fold],
                    format!("{}/*", fold),
                    None,
                    member_encrypted[fold],
                ));
            }
        } else {
            result.push((*code, path.clone(), pattern.clone(), *encrypted));
        }
        *examined += 1;
        formatting.set(*examined, None);
    }
    result
}

/// The deepest prefix that strictly contains `prefix` and has at least one
/// group path beneath it. The direct parent qualifies automatically, because
/// every path under `prefix` lies under the parent too.
fn deepest_candidate_ancestor<'a>(
    prefix: &str,
    candidates: &'a BTreeMap<String, usize>,
) -> Option<&'a String> {
    let parts: Vec<&str> = prefix.split('/').collect();
    let mut best: Option<&'a String> = None;
    let mut path = String::new();
    for (i, part) in parts.iter().enumerate() {
        if *part == "*" {
            break;
        }
        if i > 0 {
            path.push('/');
        }
        path.push_str(part);
        if i + 1 < parts.len()
            && let Some((candidate, _)) = candidates.get_key_value(&path)
        {
            best = Some(candidate);
        }
    }
    best
}

/// The outermost (shallowest) folded directory that strictly contains `path`,
/// if any. Wildcard components never belong to a fold and stop the ascent.
fn outer_fold<'a>(path: &str, folds: &'a BTreeSet<String>) -> Option<&'a String> {
    let parts: Vec<&str> = path.split('/').collect();
    let mut prefix = String::new();
    for (i, part) in parts.iter().enumerate() {
        if *part == "*" {
            break;
        }
        if i > 0 {
            prefix.push('/');
        }
        prefix.push_str(part);
        if i + 1 < parts.len()
            && let Some(fold) = folds.get(&prefix)
        {
            return Some(fold);
        }
    }
    None
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

#[allow(clippy::too_many_arguments)]
fn format_default(
    entries: &[&StatusEntry],
    all_entries: &[StatusEntry],
    stale_patterns: &[String],
    git_info: Option<&str>,
    target_dir_abs: &Path,
    source_dir_abs: &Path,
    has_managed: bool,
    show_unpulled: bool,
    formatting: &mut ActionBar,
) -> String {
    // The Unpulled block belongs only to `--unpulled`; the default report must
    // exclude it. Other commands' `--short`/`--porcelain` keep `!?` through the
    // shared filter, so the exclusion is localized here.
    let filtered: Vec<&StatusEntry> = if show_unpulled {
        entries.to_vec()
    } else {
        entries
            .iter()
            .copied()
            .filter(|e| e.code != StatusCode::Unpulled)
            .collect()
    };
    // Effective "are there entries to show" for the empty-report message.
    let no_entries = filtered.is_empty();

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
    if no_entries {
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

    for e in &filtered {
        match e.code {
            StatusCode::BothModified => merge.push(e),
            StatusCode::TargetModified => add.push(e),
            StatusCode::SourceModified => pull.push(e),
            StatusCode::Unmanaged | StatusCode::UnmanagedSymlink => unmanaged.push(e),
            StatusCode::Unpulled => unpulled.push(e),
            StatusCode::Ignored | StatusCode::IgnoredSymlink => ignored.push(e),
            StatusCode::UpToDate | StatusCode::ManagedSymlink | StatusCode::NeverSynchronized => {
                uptodate.push(e)
            }
            StatusCode::StalePattern => {}
        }
    }

    // The formatting bar spans the whole report: the step unit is one path
    // examined during collapse and display rendering. The total is unknown up
    // front, so the bar counts work units monotonically, like the walk phase.
    let mut formatting_done = 0usize;

    // Helper to write a group
    let mut write_group =
        |out: &mut String, header: &str, items: &[&StatusEntry], is_last_group: bool| {
            if items.is_empty() {
                return;
            }
            out.push_str(&format!("{}:\n", header));

            // Fold shared directories: a `dir/*` is only emitted when *every* path
            // under `dir` belongs to this group. Paths of any other status (ignored
            // pruned dirs, up-to-date / tracked files, parallel groups; plus the
            // Unpulled entries excluded from the default report) sever the fold, so
            // an ignored sibling directory keeps files listed individually.
            let member_paths: BTreeSet<&str> = items.iter().map(|i| i.path.as_str()).collect();
            let blocked: BTreeSet<String> = all_entries
                .iter()
                .filter(|e| !member_paths.contains(e.path.as_str()))
                .filter(|e| show_unpulled || e.code != StatusCode::Unpulled)
                .map(|e| e.path.clone())
                .collect();

            // Build display paths, then collapse shared directories (e.g.
            // multiple files under dir/ to a single dir/* entry). The single
            // bottom-up pass advances the formatting bar per examined path.
            let paths = collapse_shared_dirs(
                &items
                    .iter()
                    .map(|item| {
                        (
                            item.code,
                            item.path.clone(),
                            item.matched_pattern.clone(),
                            item.encrypted,
                        )
                    })
                    .collect::<Vec<_>>(),
                &blocked,
                formatting,
                &mut formatting_done,
            );

            // Build the final display list.
            struct DispLine {
                code: StatusCode,
                path: String,
                annotations: Vec<String>,
            }
            let display: Vec<DispLine> = paths
                .into_iter()
                .map(|(code, path, pattern, encrypted)| {
                    let mut annotations = Vec::new();
                    if let Some(ref pat) = pattern {
                        annotations.push(format!("({})", pat));
                    }
                    if encrypted {
                        annotations.push("(encrypted)".to_string());
                    }
                    DispLine {
                        code,
                        path,
                        annotations,
                    }
                })
                .collect();

            // Align the right-side annotations ((pattern) and (encrypted)) so they
            // all start at the same column. Lines without any annotation are not
            // padded.
            let max_path_len = display
                .iter()
                .filter(|d| !d.annotations.is_empty())
                .map(|d| d.path.len())
                .max()
                .unwrap_or(0);

            for d in &display {
                if d.annotations.is_empty() {
                    out.push_str(&format!("  {}  {}\n", d.code, color_path(d.code, &d.path)));
                } else {
                    out.push_str(&format!(
                        "  {}  {:<max_width$}  {}\n",
                        d.code,
                        color_path(d.code, &d.path),
                        d.annotations.join(" "),
                        max_width = max_path_len
                    ));
                }
            }
            // Do not print after the last group in list as 'ls -lR' shell command
            if !is_last_group {
                out.push('\n');
            }
            // The collapse pass advanced the counter per examined path; the
            // rendered lines are work too, so keep the counter moving through
            // the display pass.
            formatting_done += display.len();
            formatting.set(formatting_done, None);
        };

    let group_order = [
        merge.is_empty(),
        add.is_empty(),
        pull.is_empty(),
        unpulled.is_empty(),
        unmanaged.is_empty(),
        uptodate.is_empty(),
        ignored.is_empty(),
        stale_patterns.is_empty(),
    ];

    let mut group_lastness = vec![];
    for i in 0..group_order.len() {
        // If all from i to the right are empty
        // then ith group is the last to be printed
        let is_last = group_order.iter().skip(i + 1).all(|&se| se);
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

#[cfg(test)]
mod collapse_tests {
    use super::*;

    fn entry(path: &str) -> (StatusCode, String, Option<String>, bool) {
        (StatusCode::Unmanaged, path.to_string(), None, false)
    }

    fn collapse(
        paths: &[(StatusCode, String, Option<String>, bool)],
        blocked: &BTreeSet<String>,
    ) -> Vec<(StatusCode, String, Option<String>, bool)> {
        let mut bar = ActionBar::suppressed("formatting output");
        let mut examined = 0usize;
        collapse_shared_dirs(paths, blocked, &mut bar, &mut examined)
    }

    fn paths_of(result: &[(StatusCode, String, Option<String>, bool)]) -> Vec<&str> {
        result.iter().map(|(_, path, _, _)| path.as_str()).collect()
    }

    #[test]
    fn single_deep_group_collapses_to_outermost() {
        let paths = vec![entry("a/b/1.txt"), entry("a/b/2.txt"), entry("a/c/3.txt")];
        let result = collapse(&paths, &BTreeSet::new());
        assert_eq!(paths_of(&result), ["a/*"]);
    }

    #[test]
    fn chain_empty_except_collapsing_leaf_folds_only_at_leaf() {
        // a/b/c has two files and nothing else shares a/b or a: folding c leaves
        // both a/b and a with a single member each, so only a/b/c collapses
        // (a regression of the greedy deepest-first behaviour).
        let paths = vec![
            entry("a/b/c/f1.txt"),
            entry("a/b/c/f2.txt"),
            entry("d/e/g1.txt"),
            entry("d/e/g2.txt"),
        ];
        let result = collapse(&paths, &BTreeSet::new());
        assert_eq!(paths_of(&result), ["a/b/c/*", "d/e/*"]);
    }

    #[test]
    fn parent_with_single_foldable_child_stays_at_child() {
        // p has only a foldable child p/x; folding p/x leaves p with one member,
        // so p must not fold further (matches the previous deepest-first result).
        let paths = vec![entry("p/x/1.txt"), entry("p/x/2.txt")];
        let result = collapse(&paths, &BTreeSet::new());
        assert_eq!(paths_of(&result), ["p/x/*"]);
    }

    #[test]
    fn blocked_prefix_severs_every_ancestor() {
        // mixed/two is a different status: neither `mixed` nor anything above
        // may fold, but a sibling dir not containing the blocker still folds.
        let paths = vec![
            entry("mixed/one.txt"),
            entry("mixed/three.txt"),
            entry("plain/x.txt"),
            entry("plain/y.txt"),
        ];
        let mut blocked = BTreeSet::new();
        blocked.insert("mixed/two.txt".to_string());
        let result = collapse(&paths, &blocked);
        assert_eq!(
            paths_of(&result),
            ["mixed/one.txt", "mixed/three.txt", "plain/*"]
        );
    }

    #[test]
    fn sibling_folds_remain_independent() {
        let paths = vec![
            entry("dir1/a.txt"),
            entry("dir1/b.txt"),
            entry("dir2/c.txt"),
            entry("dir2/d.txt"),
        ];
        let result = collapse(&paths, &BTreeSet::new());
        assert_eq!(paths_of(&result), ["dir1/*", "dir2/*"]);
    }

    #[test]
    fn folded_entry_is_encrypted_only_when_all_members_are() {
        let a = entry("e/f/1.txt");
        let mut b = entry("e/f/2.txt");
        b.3 = true;
        let result = collapse(&[a, b], &BTreeSet::new());
        assert!(!result[0].3);
        let mut c = entry("e/f/3.txt");
        c.3 = true;
        let mut d = entry("e/f/4.txt");
        d.3 = true;
        let result = collapse(&[c, d], &BTreeSet::new());
        assert!(result[0].3);
    }

    #[test]
    fn root_level_files_and_unshared_dirs_are_kept() {
        let paths = vec![
            entry("root_file.txt"),
            entry("lone/single.txt"),
            entry("a/b/1.txt"),
            entry("a/b/2.txt"),
        ];
        let result = collapse(&paths, &BTreeSet::new());
        assert_eq!(
            paths_of(&result),
            ["root_file.txt", "lone/single.txt", "a/b/*"]
        );
    }

    #[test]
    fn star_component_stops_the_ascent_but_ancestors_above_still_count() {
        // The `*` component breaks the ancestor ascent (never fold across it),
        // yet the ancestor `w` above the star is still tallied just like the
        // previous implementation did, so both files fold to `w/*`.
        let paths = vec![entry("w/*/x"), entry("w/*/y")];
        let result = collapse(&paths, &BTreeSet::new());
        assert_eq!(paths_of(&result), ["w/*"]);
    }

    #[test]
    fn large_bushy_report_completes_quickly() {
        // 50k files across 10k shared dirs: the old per-fold rescan was O(n²)
        // here; the single-pass fold must stay linear.
        let mut paths = Vec::with_capacity(50_000);
        for d in 0..10_000 {
            for f in 0..5 {
                paths.push(entry(&format!("dir{d}/file{f}.txt")));
            }
        }
        let start = std::time::Instant::now();
        let result = collapse(&paths, &BTreeSet::new());
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(result.len(), 10_000);
        assert!(result.iter().all(|(_, path, _, _)| path.ends_with("/*")));
    }
}
