use std::collections::{BTreeMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command as ProcessCmd, Stdio};
use std::io::Write;

use colored::Colorize;
use log::{debug, info};

use dfm::*;
use crate::{Args, Command, DfmError};
use super::{source_rel_to_target_rel, list_directory_or_error};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct StatusEntry {
    /// Two-letter status code.
    code: &'static str,
    /// Display path (relative to target or source directory).
    path: String,
    /// Ignore pattern that matched this file (only for `!!` entries).
    matched_pattern: Option<String>,
}

// ---------------------------------------------------------------------------
// Status command entry point
// ---------------------------------------------------------------------------

pub fn status_command(settings: &Settings, args: &Args, state: &StateObject) -> Result<(), DfmError> {
    let Command::Status {
        all,
        short,
        porcelain,
        conflicted,
        modified,
        unmanaged,
        managed,
        unpulled,
        ignored,
        ignored_patterns,
        unused_patterns,
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `status`", args.command)));
    };

    let (target_dir_abs, source_dir_abs) = calc_working_dir_paths(&settings)?;

    let target_ignore_file = calc_local_ignore_file()?;
    let target_ignore_regex = load_ignore_regex(&target_ignore_file)?;

    let source_ignore_file = calc_source_ignore_file(&source_dir_abs)?;
    let _source_ignore_regex = load_ignore_regex(&source_ignore_file)?;

    // Paths to dfm's own internal files (skip in unmanaged detection)
    let state_file_path = calc_state_file_path().ok();
    let config_file_path = calc_config_file_path().ok();

    // ------------------------------------------------------------------
    // Phase 1 — Process every state entry (managed files)
    // ------------------------------------------------------------------
    let mut entries: Vec<StatusEntry> = Vec::new();
    let mut state_keys: HashSet<String> = HashSet::new();

    for (source_rel, sync_time) in &state.syncs {
        state_keys.insert(source_rel.clone());

        let source_abs = PathBuf::from_iter([source_dir_abs.to_str().unwrap(), source_rel]);
        let source_abs = remove_dots_from_path(&source_abs);

        let target_rel = source_rel_to_target_rel(
            source_rel,
            &settings.dot_prefix,
            &settings.symlink_postfix,
            &settings.encrypted_postfix,
        );
        let target_abs = PathBuf::from_iter([target_dir_abs.to_str().unwrap(), &target_rel]);
        let target_abs = remove_dots_from_path(&target_abs);

        debug!("status: state entry {:?} → target {:?}", source_rel, target_abs);

        // Check if this is a managed symlink (state key ends with symlink_postfix)
        let is_managed_symlink = source_rel.ends_with(&settings.symlink_postfix);

        // Check ignore patterns
        if let Some(pattern) = check_path_matches_regex_component_wise(&target_ignore_regex, &PathBuf::from(&target_rel)) {
            let code = if is_managed_symlink { "!L" } else { "!!" };
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
            // Managed symlink: use LL (both exist) or standard missing-file codes
            let code = if target_exists && source_exists {
                "LL"
            } else if !target_exists && source_exists {
                "!?"
            } else if target_exists && !source_exists {
                "NM"
            } else {
                debug!("status: stale state entry {:?}, both sides missing", source_rel);
                continue;
            };
            entries.push(StatusEntry { code, path: target_rel.clone(), matched_pattern: None });
            continue;
        }

        // Regular file classification via timestamp comparison
        let (code, path) = if !target_exists && source_exists {
            ("!?", target_rel.clone())
        } else if target_exists && !source_exists {
            ("NM", target_rel.clone())
        } else if target_exists && source_exists {
            let cmp = compare_files_by_timestamps(&target_abs, &source_abs, Some(sync_time))?;
            match cmp {
                CompareByTimestamp::BothModified => ("MM", target_rel.red().to_string()),
                CompareByTimestamp::TargetModified => ("M ", target_rel.yellow().to_string()),
                CompareByTimestamp::SourceModified => (" M", target_rel.yellow().to_string()),
                CompareByTimestamp::NonModified => ("--", target_rel.green().to_string()),
                CompareByTimestamp::NeverSynchronized => ("NM", target_rel.clone()),
            }
        } else {
            // Both sides missing — stale state entry, skip
            debug!("status: stale state entry {:?}, both sides missing", source_rel);
            continue;
        };

        entries.push(StatusEntry {
            code,
            path,
            matched_pattern: None,
        });
    }

    // ------------------------------------------------------------------
    // Phase 2 — Walk target directory for unmanaged files
    // ------------------------------------------------------------------
    let traversed_target = list_directory_or_error(
        &[target_dir_abs.clone()],
        None,
        "target directory for status",
    )?;

    // Phase 3 builds its own list from traversed_target + entries

    // Pre-compute canonical source dir for robust path comparison
    let canon_source_dir = fs::canonicalize(&source_dir_abs).unwrap_or_else(|_| source_dir_abs.clone());

    for target_abs in &traversed_target {
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
        if let Some(ref sfp) = state_file_path {
            if *target_abs == *sfp { continue; }
        }
        if let Some(ref cfp) = config_file_path {
            if *target_abs == *cfp { continue; }
        }
        if *target_abs == target_ignore_file {
            continue;
        }

        // Compute the relative path for the display
        let rel = file_path_relative_to(target_abs, &target_dir_abs);
        let rel = remove_dots_from_path(&rel);
        let rel_str = rel.to_str().unwrap().to_string();

        // ------------------------------------------------------------------
        // Symlink handling
        // ------------------------------------------------------------------
        if target_abs.is_symlink() {
            // Check if this symlink is managed — either directly via pointer file
            // in state, or via its resolved pointee having a managed source copy.
            let pointer_path = filepath_in_source_dir(
                &settings.dot_prefix, &target_dir_abs, &source_dir_abs,
                target_abs, Some(&settings.symlink_postfix),
            );
            let pointer_rel = file_path_relative_to(&pointer_path, &source_dir_abs);
            let pointer_rel = remove_dots_from_path(&pointer_rel);
            let pointer_in_state = state_keys.contains(pointer_rel.to_str().unwrap_or(""));

            let pointee_in_state = fs::read_link(target_abs)
                .ok()
                .and_then(|link_target| {
                    let abs = target_abs.parent().unwrap_or(std::path::Path::new(".")).join(&link_target);
                    fs::canonicalize(&abs).ok()
                })
                .map(|pointee_abs| {
                    // Only consider pointee managed when it points into the source dir
                    // (the --symlink pattern). A pointee inside the target dir or elsewhere
                    // does NOT make the symlink itself managed — if it's not in state.syncs
                    // it should appear as ?L so the user can decide to add it.
                    if pointee_abs.starts_with(&source_dir_abs) {
                        let rel = file_path_relative_to(&pointee_abs, &source_dir_abs);
                        let rel = remove_dots_from_path(&rel);
                        state_keys.contains(rel.to_str().unwrap_or(""))
                    } else {
                        false
                    }
                })
                .unwrap_or(false);

            if pointer_in_state || pointee_in_state {
                // Managed symlink — only shown with --all
                if *all {
                    entries.push(StatusEntry {
                        code: "LL",
                        path: rel_str.clone(),
                        matched_pattern: None,
                    });
                }
                continue;
            }

            // Check ignore patterns
            if let Some(pattern) = check_path_matches_regex_component_wise(&target_ignore_regex, &PathBuf::from(&rel_str)) {
                if *all || *ignored {
                    entries.push(StatusEntry {
                        code: "!L",
                        path: rel_str,
                        matched_pattern: Some(pattern),
                    });
                }
                continue;
            }

            // Unmanaged symlink
            entries.push(StatusEntry {
                code: "?L",
                path: rel_str,
                matched_pattern: None,
            });
            continue;
        }

        // ------------------------------------------------------------------
        // Regular file handling
        // ------------------------------------------------------------------

        // Check if this target file is already in state (via source-rel mapping)
        let source_abs = filepath_in_source_dir(
            &settings.dot_prefix, &target_dir_abs, &source_dir_abs,
            target_abs, None,
        );
        let source_rel = file_path_relative_to(&source_abs, &source_dir_abs);
        let source_rel = remove_dots_from_path(&source_rel);
        let source_rel_str = source_rel.to_str().unwrap().to_string();

        // Also try encrypted/symlink postfix variants
        let enc_key = format!("{}{}", source_rel_str, settings.encrypted_postfix);
        let sym_key = format!("{}{}", source_rel_str, settings.symlink_postfix);

        if state_keys.contains(&source_rel_str)
            || state_keys.contains(&enc_key)
            || state_keys.contains(&sym_key)
        {
            continue; // already in state — covered by Phase 1
        }

        // Check ignore
        if let Some(pattern) = check_path_matches_regex_component_wise(&target_ignore_regex, &PathBuf::from(&rel_str)) {
            if *all || *ignored {
                entries.push(StatusEntry {
                    code: "!!",
                    path: rel_str,
                    matched_pattern: Some(pattern),
                });
            }
            continue;
        }

        // Unmanaged regular file
        entries.push(StatusEntry {
            code: "??",
            path: rel_str,
            matched_pattern: None,
        });
    }

    // ------------------------------------------------------------------
    // Phase 3 — Find unused ignore patterns
    // ------------------------------------------------------------------
    let mut stale_patterns: Vec<String> = Vec::new();

    // Build a flat list of all relative paths (for component-wise pattern matching)
    let all_relative_paths: Vec<String> = {
        let mut p = Vec::new();
        // Add all traversed target files (as relative to target dir)
        for abs in &traversed_target {
            if abs.to_str().is_some() {
                let rel = file_path_relative_to(abs, &target_dir_abs);
                if let Some(rs) = rel.to_str() {
                    p.push(rs.to_string());
                }
            }
        }
        // Add all target paths from state entries (already relative)
        for entry in &entries {
            if entry.code != "!?" {
                p.push(entry.path.clone());
            }
        }
        p
    };

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

    // ------------------------------------------------------------------
    // Special mode: only list patterns
    // ------------------------------------------------------------------
    if *ignored_patterns {
        for p in target_ignore_regex.patterns() {
            println!("{}", p);
        }
        return Ok(());
    }

    if *unused_patterns {
        if stale_patterns.is_empty() {
            info!("unused ignore patterns");
        } else {
            for p in &stale_patterns {
                println!("!P\t{}", p.red().to_string());
            }
        }
        return Ok(());
    }

    // ------------------------------------------------------------------
    // Apply filters
    // ------------------------------------------------------------------
    let filtered: Vec<&StatusEntry> = entries.iter().filter(|e| {
        if *conflicted && e.code != "MM" { return false; }
        if *modified && !e.code.contains('M') { return false; }
        if *unmanaged && e.code != "??" && e.code != "?L" { return false; }
        if *managed && e.code != "--" && e.code != "MM" && e.code != "M " && e.code != " M" && e.code != "NM" && e.code != "!?" && e.code != "LL" { return false; }
        if *unpulled && e.code != "!?" { return false; }
        if *ignored && e.code != "!!" && e.code != "!L" { return false; }
        if !*all && !*ignored && (e.code == "!!" || e.code == "!L") { return false; }
        if !*all && !*managed && (e.code == "--" || e.code == "LL") { return false; }
        true
    }).collect();

    // ------------------------------------------------------------------
    // Output
    // ------------------------------------------------------------------
    let git_info = get_git_info(&source_dir_abs);

    if *porcelain {
        // Tab-separated, stable, never paged
        for entry in &filtered {
            println!("{}\t{}", entry.code, entry.path);
        }
        if *unused_patterns || filtered.is_empty() {
            // If we have stale patterns, output them too
            for p in &stale_patterns {
                println!("!P\t{}", p);
            }
        }
    } else if *short {
        for entry in &filtered {
            println!("{} {}", entry.code, entry.path);
        }
    } else {
        let has_managed = entries.iter().any(|e| matches!(
            e.code, "--" | "MM" | "M " | " M" | "NM" | "!?" | "LL"
        ));
        let output = format_default(&filtered, &stale_patterns, git_info.as_deref(), &target_dir_abs, &source_dir_abs, has_managed);
        print_paged(&output, false)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Default categorized output
// ---------------------------------------------------------------------------

fn format_default(entries: &[&StatusEntry], stale_patterns: &[String], git_info: Option<&str>, target_dir_abs: &PathBuf, source_dir_abs: &PathBuf, has_managed: bool) -> String {
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
            out.push_str("All clear.\n");
        } else {
            out.push_str("No files managed.\n");
        }
    } else {
        out.push_str("\n");
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
            "MM" => merge.push(e),
            "M " => add.push(e),
            " M" => pull.push(e),
            "??" => unmanaged.push(e),
            "?L" => unmanaged.push(e),
            "!?" => unpulled.push(e),
            "!!" | "!L" => ignored.push(e),
            "--" | "LL" => uptodate.push(e),
            _ => {}
        }
    }

    // Helper to write a group
    let write_group = |out: &mut String, header: &str, items: &[&StatusEntry], is_last_goup: bool| {
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("{}:\n", header));

        // Group by first path component so we can collapse directories with ≥2 entries.
        let mut groups: BTreeMap<&str, Vec<&&StatusEntry>> = BTreeMap::new();
        for item in items {
            let key = match item.path.find('/') {
                Some(pos) => &item.path[..pos],
                None => &item.path[..],
            };
            groups.entry(key).or_default().push(item);
        }

        // Build the final display list (collapsed + individual entries).
        struct DispLine {
            code: &'static str,
            path: String,
            pattern: Option<String>,
        }
        let mut display: Vec<DispLine> = Vec::new();

        for (_key, g) in &groups {
            let has_nested = g.iter().any(|item| item.path.contains('/'));
            if g.len() >= 2 && has_nested {
                // Collapse: show directory/* instead of every file inside it
                display.push(DispLine {
                    code: g[0].code,
                    path: format!("{}/*", _key),
                    pattern: None,
                });
            } else {
                for item in g {
                    display.push(DispLine {
                        code: item.code,
                        path: item.path.clone(),
                        pattern: item.matched_pattern.clone(),
                    });
                }
            }
        }

        // Align parenthesised patterns (matched_pattern) so '(' starts at the same column.
        let max_path_len = display
            .iter()
            .filter_map(|d| d.pattern.as_ref().map(|_| d.path.len()))
            .max()
            .unwrap_or(0);

        for d in &display {
            if let Some(ref pat) = d.pattern {
                out.push_str(&format!("  {}  {:<max_width$}  ({})\n", d.code, d.path, pat, max_width = max_path_len));
            } else {
                out.push_str(&format!("  {}  {}\n", d.code, d.path));
            }
        }
        // Do not print after the last group in list as 'ls -lR' shell command
        if !is_last_goup {
            out.push('\n');
        }
    };

    let group_order = vec![
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
            out.push_str(&format!("  !P  {}\n", p));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Pager
// ---------------------------------------------------------------------------

fn print_paged(output: &str, _force_pager: bool) -> Result<(), DfmError> {
    // Detect terminal height
    let line_count = output.lines().count();
    let term_height = terminal_height().unwrap_or(24);

    if line_count > term_height {
        // Use pager
        // TODO create a field in config file and settings for the paging command
        // by default in config it must be 'less -FRSX', but env PAGER has the priority.
        let pager_cmd = env::var("PAGER").unwrap_or_else(|_| "less -FRSX".to_string());
        let parts: Vec<&str> = pager_cmd.split_whitespace().collect();
        if parts.is_empty() {
            print!("{}", output);
            return Ok(());
        }
        let (prog, args) = parts.split_first().unwrap();
        let mut child = ProcessCmd::new(prog)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(output.as_bytes())?;
            stdin.flush()?;
        }
        child.wait()?;
    } else {
        print!("{}", output);
    }
    Ok(())
}

fn terminal_height() -> Option<usize> {
    // Try via `stty size`
    if let Ok(output) = ProcessCmd::new("stty").arg("size").stdout(Stdio::piped()).stderr(Stdio::null()).output() {
        if let Ok(s) = String::from_utf8(output.stdout) {
            if let Some(rows_str) = s.split_whitespace().next() {
                if let Ok(rows) = rows_str.parse::<usize>() {
                    return Some(rows);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Git integration
// ---------------------------------------------------------------------------

fn get_git_info(source_dir: &PathBuf) -> Option<String> {
    // Check if source_dir is a git repo
    let output = ProcessCmd::new("git")
        .args(["-C", source_dir.to_str().unwrap(), "rev-parse", "--abbrev-ref", "HEAD"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        return None;
    }

    // Check if dirty
    let status_output = ProcessCmd::new("git")
        .args(["-C", source_dir.to_str().unwrap(), "status", "--porcelain"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    let dirty = !status_output.stdout.is_empty();
    let state = if dirty { "dirty" } else { "clean" };

    Some(format!("branch: {}, {}", branch, state))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
