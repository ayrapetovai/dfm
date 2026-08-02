pub mod crypt;

use std::collections::HashMap;
use std::{fs, io};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::ops::Add;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use envmnt::ExpandOptions;
use log::{debug, trace};
use microxdg::Xdg;
use regex::{Regex, RegexSet};
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use toml::{Table, Value};
use walkdir::{DirEntry, WalkDir};
use lazy_static::lazy_static;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum DfmError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("Unsupported: {0}")]
    Unsupported(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("{0}")]
    Other(String),
}

impl From<toml::ser::Error> for DfmError {
    fn from(e: toml::ser::Error) -> Self {
        DfmError::Other(e.to_string())
    }
}

impl From<toml::de::Error> for DfmError {
    fn from(e: toml::de::Error) -> Self {
        DfmError::Other(e.to_string())
    }
}

impl DfmError {
    /// Shorthand for creating an `Other` variant.
    pub fn other(msg: impl std::fmt::Display) -> Self {
        DfmError::Other(msg.to_string())
    }
}

impl From<regex::Error> for DfmError {
    fn from(e: regex::Error) -> Self {
        DfmError::Other(e.to_string())
    }
}

impl From<microxdg::XdgError> for DfmError {
    fn from(e: microxdg::XdgError) -> Self {
        DfmError::other(e)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StateObject {
    pub source_directory: PathBuf,
    pub target_directory: PathBuf,
    pub syncs: HashMap<String, SyncTime>,
}

static STATE_DIRECTORY_NAME_IN_XDG_STATE: &str = "dfm";
static STATE_FILE_NAME_IN_XDG_STATE: &str = "state.toml";

static CONFIG_FILE_NAME_IN_HOME: &str = ".dfm.toml";
static CONFIG_FILE_NAME_IN_XDG_CONFIG: &str = "config.toml";

static IGNORE_FILE_NAME_IN_XDG_STATE : &str = "ignore_file";
static IGNORE_FILE_NAME_IN_SOURCE_DIR: &str = ".dfm_ignore_file";

/// Sentinel child name appended to a directory rel path to probe whether the
/// directory (as a non-last component) is fully ignored.
pub const IGNORE_DIR_PROBE_CHILD: &str = "x";
/// Sentinel leading `.` component (e.g. `./file.txt`) skipped in matching.
pub const LEADING_DOT_COMPONENT: &str = ".";

lazy_static! {
    // file name must be relative to target directory
    static ref BY_DEFAULT_FORCE_ENCRYPTION_FILES: Vec<Regex> = vec![Regex::from_str("\\.ssh").unwrap()];
}

impl StateObject {
    pub fn new(target_directory: PathBuf, source_directory: PathBuf) -> Self {
       StateObject {
           source_directory,
           target_directory,
           syncs: HashMap::new()
       }
    }
}

/// A sync record: the mtime assigned to both sides after a successful sync,
/// plus a SHA-256 hash of the synced content. Content hashing makes change
/// detection deterministic — it does not depend on filesystem mtime
/// granularity.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncTime {
    #[serde(with = "sync_time_ser")]
    pub mtime: SystemTime,
    pub sha256: String,
}

impl std::ops::Deref for SyncTime {
    type Target = SystemTime;
    fn deref(&self) -> &SystemTime { &self.mtime }
}

mod sync_time_ser {
    use serde::{de::Error as DeError, ser::Error as SerError, Deserialize, Deserializer, Serializer};
    use std::time::SystemTime;

    pub fn serialize<S: Serializer>(t: &SystemTime, s: S) -> Result<S::Ok, S::Error> {
        let dur = t.duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| S::Error::custom("timestamp is before UNIX epoch"))?;
        let line = format!("{};{}", dur.as_secs(), dur.subsec_nanos());
        s.serialize_str(&line)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<SystemTime, D::Error> {
        let line = <String as Deserialize>::deserialize(d)?;
        let (secs, nanos) = line.split_once(';')
            .ok_or_else(|| D::Error::custom("expected \"<secs>;<nanos>\""))?;
        let secs: u64 = secs.parse().map_err(D::Error::custom)?;
        let nanos: u32 = nanos.parse().map_err(D::Error::custom)?;
        let dur = std::time::Duration::new(secs, nanos);
        Ok(SystemTime::UNIX_EPOCH + dur)
    }
}

pub fn calc_local_ignore_file(xdg: &Xdg) -> Result<PathBuf, DfmError> {
    let state_file_name = format!("{}/{}", STATE_DIRECTORY_NAME_IN_XDG_STATE, IGNORE_FILE_NAME_IN_XDG_STATE);
    Ok(xdg.state_file(&state_file_name)?)
}

pub fn open_or_create_target_ignore_file(xdg: &Xdg) -> Result<File, DfmError> {
    let state_file_name = format!("{}/{}", STATE_DIRECTORY_NAME_IN_XDG_STATE, IGNORE_FILE_NAME_IN_XDG_STATE);
    let p = xdg.state_file(&state_file_name)?;
    Ok(OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(&p)?)
}

pub fn open_or_create_file(path_to_file: &PathBuf) -> Result<File, DfmError> {
    Ok(OpenOptions::new()
        .write(true)
        .append(true)
        .create(true)
        .open(path_to_file)?)
}

// TODO refactor, make less code
pub fn calc_source_ignore_file(source_dir_abs_path: &PathBuf) -> Result<PathBuf, DfmError> {
    let source_ignore_file_path = source_dir_abs_path.join(IGNORE_FILE_NAME_IN_SOURCE_DIR);
    Ok(source_ignore_file_path)
}

pub fn load_ignore_regex(ignore_file_path : &PathBuf) -> Result<RegexSet, DfmError> {
    if !ignore_file_path.exists() {
        return Ok(RegexSet::empty());
    }

    let file = File::open(ignore_file_path)?;
    let reader = BufReader::new(file);
    let mut patterns = vec![];

    for (_, line) in reader.lines().enumerate() {
        let line = line?;
        let mut prev = ' ';
        let mut end = line.len();
        for (i, c) in line.char_indices() {
            if c == '#' && prev != '\\' {
                end = i;
                break;
            }
            prev = c;
        }
        let line = line[0..end].to_owned();
        if !line.is_empty() {
            patterns.push(line)
        }
    }

    return if patterns.is_empty() {
        Ok(RegexSet::empty())
    } else {
        match RegexSet::new(patterns) {
            Ok(r) => Ok(r),
            Err(e) => Err(DfmError::other(e))
        }
    }
}

pub fn check_path_matches_regex(regex: &RegexSet, haystack: &PathBuf) -> Option<String> {
    let haystack = haystack.to_string_lossy();
    if regex.matches(haystack.as_ref()).matched_any() {
        let target_ignore_patterns = regex.patterns();
        for pattern in target_ignore_patterns {
            let regex = Regex::new(pattern).unwrap();
            if regex.is_match(haystack.as_ref()) {
                return Some(pattern.to_owned());
            }
        }
    }
    return None;
}

/// Check if a regex pattern matches a relative path when matching is done
/// component-wise (between `/` separators) with implicit anchoring at each
/// component boundary.
///
/// * `pattern` — the regex pattern (may contain `/` to span multiple components)
/// * `relative_path` — path relative to target or source directory
///   (no leading `/`)
///
/// # Matching rules
///
/// * The pattern is split on `/` into sub-patterns.
/// * The path is split on `/` into components.
/// * Each sub-pattern is tested against a single component.
/// * If the sub-pattern doesn't already have `^` or `$` anchors, it is
///   implicitly anchored at both ends (`^(?:…)$`) so it must match the
///   entire component. If it does have anchors, they are respected as-is.
/// * A single sub-pattern matches when any path component matches.
/// * Multiple sub-patterns match when adjacent path components match each
///   sub-pattern in order (sliding window).
///
/// # Examples
///
/// | Pattern | Relative path | Matches? |
/// |---|---|---|
/// | `.*abc\.c` | `dir/abc.c` | ✅ — component `abc.c` |
/// | `.*abc\.c` | `dir/the-abc.c` | ✅ — component `the-abc.c` |
/// | `abc\.c` | `dir/abc.c` | ✅ — exactly component `abc.c` |
/// | `abc\.c` | `dir/the-abc.c` | ❌ — not exact |
/// | `.*abc/def.*` | `1abc/define/123` | ✅ — adjacent `1abc`+`define` |
/// | `.*abc/def.*` | `abc/something/define` | ❌ — not adjacent |
pub fn pattern_matches_path_components(pattern: &str, relative_path: &str) -> bool {
    // A trailing '/' in either the pattern or the path (e.g. "dirname/") yields
    // an empty trailing segment that can never match a real component, making
    // patterns like "dirname/" dead. Drop trailing empties so "dirname/"
    // behaves like "dirname".
    let mut components: Vec<&str> = relative_path.split('/').collect();
    while components.last() == Some(&"") {
        components.pop();
    }

    let mut sub_patterns: Vec<&str> = pattern.split('/').collect();
    while sub_patterns.last() == Some(&"") {
        sub_patterns.pop();
    }

    if sub_patterns.is_empty() {
        return false;
    }

    // Anchor the sub-pattern to match the entire component UNLESS it already
    // has explicit anchors (^ or $).
    let anchored = |p: &str| -> String {
        if p.starts_with('^') || p.ends_with('$') {
            p.to_string()
        } else {
            format!("^(?:{})$", p)
        }
    };

    if sub_patterns.len() == 1 {
        let sub = sub_patterns[0];
        // A leading glob (.* or ^) means "match any component at any depth".
        let has_left_glob = sub.starts_with(".*") || sub.starts_with('^');
        if has_left_glob {
            // Wildcard at left — test every component
            if let Ok(re) = Regex::new(&anchored(sub)) {
                for comp in &components {
                    if re.is_match(comp) {
                        return true;
                    }
                }
            }
            false
        } else {
            // No wildcard at left — match any component that is NOT the last
            // component (i.e., a directory prefix). A root-level path
            // (single component, possibly prefixed with "./") always matches.
            if let Ok(re) = Regex::new(&anchored(sub)) {
            // Skip a leading "." component ("./file.txt" → "file.txt")
            let comps: &[&str] = if components.first() == Some(&LEADING_DOT_COMPONENT) {
                &components[1..]
            } else {
                &components[..]
            };
                if comps.len() == 1 {
                    // Root-level: match the single component
                    re.is_match(comps[0])
                } else {
                    // Match any component except the last (filename) one
                    comps[..comps.len() - 1].iter().any(|c| re.is_match(c))
                }
            } else {
                false
            }
        }
    } else {
        // Multiple sub-patterns: try sliding window of adjacent components
        if sub_patterns.len() > components.len() {
            return false;
        }
        let sub_res: Vec<Option<Regex>> = sub_patterns
            .iter()
            .map(|p| Regex::new(&anchored(p)).ok())
            .collect();
        if sub_res.iter().any(|r| r.is_none()) {
            return false;
        }
        for window in components.windows(sub_patterns.len()) {
            let mut all_match = true;
            for (i, comp) in window.iter().enumerate() {
                if let Some(ref re) = sub_res[i] {
                    if !re.is_match(comp) {
                        all_match = false;
                        break;
                    }
                } else {
                    all_match = false;
                    break;
                }
            }
            if all_match {
                return true;
            }
        }
        false
    }
}

/// Version of `check_path_matches_regex` that uses component-wise matching
/// (path between `/` separators) instead of full-path substring matching.
///
/// `haystack` should be a path **relative** to the directory whose ignore
/// file is being checked (target or source).
pub fn check_path_matches_regex_component_wise(
    regex_set: &RegexSet,
    haystack: &PathBuf,
) -> Option<String> {
    let haystack_str = haystack.to_string_lossy();
    if !regex_set.matches(haystack_str.as_ref()).matched_any() {
        return None;
    }
    for pattern in regex_set.patterns() {
        if pattern_matches_path_components(pattern, haystack_str.as_ref()) {
            return Some(pattern.to_owned());
        }
    }
    None
}

pub fn calc_state_directory_path(xdg: &Xdg) -> Result<PathBuf, DfmError> {
    Ok(xdg.state()?.join(STATE_DIRECTORY_NAME_IN_XDG_STATE))
}

pub fn calc_state_file_path(xdg: &Xdg) -> Result<PathBuf, DfmError> {
    let state_file_name = format!("{}/{}", STATE_DIRECTORY_NAME_IN_XDG_STATE, STATE_FILE_NAME_IN_XDG_STATE);
    Ok(xdg.state_file(&state_file_name)?)
}

pub fn read_state(path_to_state_file: &PathBuf) -> Result<StateObject, DfmError> {
    trace!("state file path {:?}", path_to_state_file);

    let state_file_content = match fs::read_to_string(path_to_state_file) {
        Ok(s) => s,
        Err(e) => {
            return Err(DfmError::other(e));
        }
    };

    return match toml::from_str(&state_file_content) {
        Err(e) => {
            return Err(DfmError::other(e));
        },
        Ok(s) => Ok(s)
    };
}

pub fn write_state(path_to_state_file: &PathBuf, state: &StateObject) -> Result<(), DfmError> {
    let state_content = toml::to_string_pretty(state)?;
    Ok(fs::write(path_to_state_file, state_content)?)
}

/// Config read from the TOML file on disk (all fields optional).
#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub dot_prefix: Option<String>,
    pub symlink_postfix: Option<String>,
    pub encrypted_postfix: Option<String>,
    // pub compare_content: Option<bool>, compare files by content

    #[serde(with = "serde_regex")]
    pub force_encryption_for: Vec<Regex>,
    pub obtain_password_shell_command: Option<String>,
    pub merge_tool_command: Option<String>,
}

impl Config {
    pub fn from_settings(settings: &Settings) -> Self {
        Config {
            dot_prefix: Some(settings.dot_prefix.clone()),
            symlink_postfix: Some(settings.symlink_postfix.clone()),
            encrypted_postfix: Some(settings.encrypted_postfix.clone()),
            force_encryption_for: settings.force_encryption_for.clone(),
            obtain_password_shell_command: settings.obtain_password_shell_command.clone(),
            merge_tool_command: settings.merge_tool_command.clone(),
        }
    }
}

/// Runtime settings after merging defaults + config file + state.
#[derive(Debug, Clone)]
pub struct Settings {
    pub config_file_found: bool,
    pub source_dir: String,
    pub target_dir: String,
    pub dot_prefix: String,
    pub symlink_postfix: String,
    pub encrypted_postfix: String,
    // pub compare_content: Option<bool>, compare files by content

    pub force_encryption_for: Vec<Regex>,
    pub obtain_password_shell_command: Option<String>,
    pub merge_tool_command: Option<String>,
}

pub fn write_config(path_to_config_file: &PathBuf, config: &Config) -> Result<(), DfmError> {
    let content = match toml::to_string_pretty(config) {
        Ok(c) => c,
        Err(e) => {
            return Err(DfmError::other(e));
        }
    };
    if let Some(config_parent_directory) = path_to_config_file.parent() {
        if !config_parent_directory.exists() {
            fs::create_dir_all(config_parent_directory)?;
        }
    }
    Ok(fs::write(path_to_config_file, content)?)
}

// TODO read HOME variable depending on the operation system
// [dependencies]
// env_home = "0.1"
pub fn get_home_path() -> Option<PathBuf> {
    if !envmnt::exists("HOME") {
        return None;
    }
    let mut expand_options = ExpandOptions::new();
    expand_options.default_to_empty = true;
    let home_path = envmnt::expand("${HOME}", Some(expand_options));
    return if home_path.len() > 0 {
        Some(PathBuf::from(home_path))
    } else {
        None
    }
}

pub fn calc_config_file_path(xdg: &Xdg) -> Result<PathBuf, DfmError>{
    let home_path = match get_home_path() {
        Some(p) => p,
        None => return Err(DfmError::Unsupported("Environment variable $HOME is not set".into()))
    };
    let config_in_home = home_path.join(CONFIG_FILE_NAME_IN_HOME);

    let path_to_config_file = match xdg.config() {
        Ok(path_to_config_dir) => {
            let state_file_name = format!("{}/{}", STATE_DIRECTORY_NAME_IN_XDG_STATE, CONFIG_FILE_NAME_IN_XDG_CONFIG);
            let config_path = path_to_config_dir.join(&state_file_name);
            if config_path.exists() || !config_in_home.exists() {
                trace!("config file path is taken from XDG variable {:?}", config_path);
                config_path
            } else {
                trace!("config file was not found {:?}", config_path);
                config_in_home
            }
        },
        Err(e) => {
            trace!("xdg config path is absent: {}", e);
            config_in_home
        }
    };

    return Ok(path_to_config_file);
}

pub fn create_default_settings() -> Settings {
    Settings {
        config_file_found: false,
        source_dir: "".to_owned(),
        target_dir: "$HOME".to_owned(), // TODO read HOME depending on operating system
        dot_prefix: "dot_".to_owned(),
        symlink_postfix: ".symlink".to_owned(),
        encrypted_postfix: ".encrypted".to_owned(),
        force_encryption_for: BY_DEFAULT_FORCE_ENCRYPTION_FILES.to_vec(),
        obtain_password_shell_command: Some("".to_owned()), // TODO need to make serde to add empty files to file
        merge_tool_command: Some("vimdiff {target} {source} {result}".to_owned()),
    }
}

pub fn read_config(path_to_config_file: &PathBuf) -> Result<Config, DfmError> {
    trace!("config file path {:?}", path_to_config_file);

    let config_file_content = match fs::read_to_string(path_to_config_file) {
        Ok(s) => s,
        Err(e) => {
            return Err(DfmError::other(e));
        }
    };

    return match toml::from_str(&config_file_content) {
        Err(e) => {
            return Err(DfmError::other(e));
        },
        Ok(c) => Ok(c)
    };
}

pub fn merge_settings(default: &Settings, custom_opt: &Option<Config>, state_object: Option<&StateObject>) -> Settings {
    match custom_opt {
        Some(custom) =>
            Settings {
                config_file_found: true,
                source_dir: match state_object {
                    Some(state) => state.source_directory.to_string_lossy().into_owned(),
                    None => "".to_string()
                },
                target_dir: match state_object {
                    Some(state) => state.target_directory.to_string_lossy().into_owned(),
                    None => "".to_string()
                },
                dot_prefix: match &custom.dot_prefix {
                    Some(v) => v.clone(),
                    None => default.dot_prefix.to_string()
                },
                symlink_postfix: match &custom.symlink_postfix {
                    Some(v) => v.clone(),
                    None => default.symlink_postfix.to_string()
                },
                encrypted_postfix: match &custom.encrypted_postfix {
                    Some(v) => v.clone(),
                    None => default.encrypted_postfix.to_string()
                },
                force_encryption_for: if !custom.force_encryption_for.is_empty() {
                    custom.force_encryption_for.clone()
                } else {
                    default.force_encryption_for.clone()
                },
                obtain_password_shell_command: match &custom.obtain_password_shell_command {
                    Some(s) => Some(s.clone()),
                    None => default.obtain_password_shell_command.clone()
                },
                merge_tool_command: match &custom.merge_tool_command {
                    Some(s) => Some(s.clone()),
                    None => default.merge_tool_command.clone()
                },
            },
        None => default.clone()
    }
}

pub fn file_path_relative_to(file_abs_path: &PathBuf, relative_to_abs_path: &PathBuf) -> PathBuf {
    let mut target_file_rel_to_target_dir_path_opt: Option<PathBuf> = None;
    let mut path_components = Vec::new();
    for target_file_parent in file_abs_path.ancestors() {
        if relative_to_abs_path.eq(target_file_parent) {
            target_file_rel_to_target_dir_path_opt = Some(PathBuf::from_iter(&path_components));
            break;
        }
        if let Some(filename) = target_file_parent.file_name() {
            path_components.insert(0, filename);
        }
    }

    if let Some(ret) = target_file_rel_to_target_dir_path_opt {
        return if ret.as_os_str().is_empty() { PathBuf::from(".") } else { ret };
    } else {
        let mut target_file_rel_to_target_dir_path_with_backs = file_abs_path.to_string_lossy().into_owned();
        for _ in 0..path_components.len() {
            target_file_rel_to_target_dir_path_with_backs.insert_str(0, "/..")
        }
        target_file_rel_to_target_dir_path_with_backs.insert_str(0, ".");
        PathBuf::from(target_file_rel_to_target_dir_path_with_backs)
    }
}

pub fn filepath_in_source_dir(dot_prefix: &str, target_dir_abs_path: &PathBuf, source_dir_abs_path: &PathBuf, target_abs_path: &PathBuf, add_postfix_opt: Option<&str>) -> PathBuf {
    let regexp_for_leading_dot_in_filename = Regex::new(r#"^\."#).unwrap();
    let regexp_for_leading_dot_in_path = Regex::new(r#"/\.[^.]"#).unwrap();

    let slash_dot_prefix = String::from_iter(vec!["/", &dot_prefix]);

    let target_file_rel_to_target_dir_path = file_path_relative_to(target_abs_path, &target_dir_abs_path);

    trace!("target file path relative to target directory {:?}", target_file_rel_to_target_dir_path);

    // replace dots in filenames and dirnames to dot_prefix from config
    let filename = regexp_for_leading_dot_in_filename.replace(&target_file_rel_to_target_dir_path.file_name().unwrap().to_string_lossy(), dot_prefix).to_string();
    let parent = regexp_for_leading_dot_in_filename.replace(&target_file_rel_to_target_dir_path.parent().unwrap().to_string_lossy(), dot_prefix).to_string();
    let mut dirname = regexp_for_leading_dot_in_path.replace_all(&parent, &slash_dot_prefix).to_string();
    if !dirname.is_empty() {
        dirname.push('/');
    }
    else {
        dirname.push_str("./");
    }

    let mut source_file_rel_to_source_dir_path = String::from_iter(vec![dirname, filename]);
    if let Some(postfix) = add_postfix_opt {
        source_file_rel_to_source_dir_path = source_file_rel_to_source_dir_path.add(postfix);
    }

    trace!("source file path relative to source directory {}", source_file_rel_to_source_dir_path);
    let ret = source_dir_abs_path.join(&source_file_rel_to_source_dir_path);
    return remove_dots_from_path(&ret);
}

pub fn remove_dots_from_path(path: &Path) -> PathBuf {
    let absolute = path.is_absolute();
    let mut stack: Vec<OsString> = Vec::new();
    let mut pops_above_root = 0usize;

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match stack.pop() {
                Some(_) => {}
                None => pops_above_root += 1,
            },
            Component::Normal(name) => stack.push(name.to_os_string()),
            Component::RootDir | Component::Prefix(_) => {}
        }
    }

    let mut ret = PathBuf::new();
    if absolute && pops_above_root == 0 {
        ret.push("/");
    }
    for _ in 0..pops_above_root {
        ret.push("..");
    }
    for component in stack {
        ret.push(component);
    }
    if ret.as_os_str().is_empty() {
        ret.push(".");
    }
    ret
}

pub fn calc_working_dir_paths(settings: &Settings) -> Result<(PathBuf, PathBuf), DfmError> {
    let (target_dir_abs_path, source_dir_abs_path) = calc_working_dir_paths_unchecked(settings)?;

    if !source_dir_abs_path.is_dir() {
        return Err(DfmError::other(format!(
            "source directory does not exist: {}. Run `dfm init` to create it.",
            source_dir_abs_path.display()
        )));
    }

    Ok((target_dir_abs_path, source_dir_abs_path))
}

pub fn calc_working_dir_paths_unchecked(settings: &Settings) -> Result<(PathBuf, PathBuf), DfmError> {
    if settings.source_dir.trim().is_empty() {
        return Err(DfmError::other("failed to read source path from the config file: empty string"));
    }

    trace!("using target directory from settings (original) {:?}", settings.target_dir);

    let target_dir_path_expanded = envmnt::expand(&settings.target_dir, None);
    trace!("using target directory from settings (expanded) {}", target_dir_path_expanded);

    let target_dir_abs_path = match PathBuf::from_str(target_dir_path_expanded.as_str()) {
        Ok(p) => remove_dots_from_path(&p),
        Err(e) => return Err(DfmError::other(e))
    };

    trace!("using source directory from settings (original) {:?}", settings.source_dir);

    let source_dir_path_expanded = envmnt::expand(&settings.source_dir, None);
    trace!("using source directory from settings (expanded) {}", source_dir_path_expanded);

    let source_dir_abs_path = match PathBuf::from_str(source_dir_path_expanded.as_str()) {
        Ok(p) => remove_dots_from_path(&p),
        Err(e) => return Err(DfmError::other(e)),
    };

    return Ok((target_dir_abs_path, source_dir_abs_path));
}

// ---------------------------------------------------------------------------
// Single-line progress indicator
// ---------------------------------------------------------------------------

/// Renders a self-overwriting progress line on stderr.
///
/// Each `set` overwrites the previous line in place (carriage return + space
/// padding), so a long-running operation shows one updating line instead of
/// many lines. `clear` (also run automatically on drop) erases the line, so
/// nothing lingers after the operation finishes.
pub struct ProgressLine {
    last_len: usize,
    active: bool,
}

impl ProgressLine {
    pub fn new() -> ProgressLine {
        ProgressLine { last_len: 0, active: false }
    }

    /// Replace the current progress line with `text`, overwriting in place.
    ///
    /// No-op when info-level logging is enabled (`-v > 1`): stderrlog maps
    /// `-v 2`/`-v 3` to `log::LevelFilter::Info`/`Debug`, and that log output
    /// would interleave with the self-overwriting progress line, producing
    /// garbled terminal output. Errors and warnings (`-v 0`/`-v 1`) never
    /// spam mid-operation, so progress stays rendered there.
    pub fn set(&mut self, text: &str) {
        if log::max_level() >= log::LevelFilter::Info {
            return;
        }
        use std::io::Write;
        let mut stderr = std::io::stderr();
        let _ = write!(stderr, "\r{}", text);
        if text.len() < self.last_len {
            let _ = write!(stderr, "{}", " ".repeat(self.last_len - text.len()));
        }
        let _ = stderr.flush();
        self.last_len = text.len();
        self.active = true;
    }

    /// Erase the progress line if one is currently shown.
    pub fn clear(&mut self) {
        use std::io::Write;
        if self.active {
            let mut stderr = std::io::stderr();
            let _ = write!(stderr, "\r{}", " ".repeat(self.last_len));
            let _ = write!(stderr, "\r");
            let _ = stderr.flush();
            self.active = false;
        }
    }
}

impl Default for ProgressLine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ProgressLine {
    fn drop(&mut self) {
        self.clear();
    }
}

#[derive(Debug)]
pub struct ListDirectories {
    pub found: Vec<PathBuf>,
    pub errors: Vec<String>,
    /// Relative paths (to `rel_base`) of directories pruned by the traversal
    /// filter. Only populated by `TraversalFilter::PruneIgnoredDirs`.
    pub pruned: Vec<String>,
}

/// Decides which entries the directory walk should keep.
///
/// Rel paths are computed relative to `rel_base` (the target or source
/// directory the ignore patterns are relative to). The walk root is always
/// kept, so an explicitly named ignored path is still entered.
#[derive(Clone, Copy, Debug)]
pub enum TraversalFilter<'a> {
    /// Keep an entry iff its rel path matches the regex. Applies to both
    /// files and directories (used by `pull` to skip dotfiles/dot-dirs).
    KeepMatching(&'a RegexSet),
    /// Keep every file, but prune directories whose subtree is fully ignored
    /// (component-wise matching). Pruned dirs are recorded in
    /// `ListDirectories::pruned` (used by `add`/`forget`/`status`/`ignore`).
    PruneIgnoredDirs(&'a RegexSet),
}

/// Whether a directory at rel path `dir_rel` is fully ignored by `regex`.
///
/// `pattern_matches_path_components` treats the *last* component as a
/// filename, so the directory is tested as a non-last component by appending
/// a sentinel (which never matches a real sub-pattern unless it is a glob
/// that matches everything — and then pruning is still correct).
pub fn is_dir_ignored(regex: &RegexSet, dir_rel: &str) -> bool {
    dir_ignore_pattern(regex, dir_rel).is_some()
}

/// Pattern that fully ignores the directory at rel path `dir_rel`, probed as
/// `dir_rel/IGNORE_DIR_PROBE_CHILD`. `None` when not ignored.
pub fn dir_ignore_pattern(regex: &RegexSet, dir_rel: &str) -> Option<String> {
    check_path_matches_regex_component_wise(
        regex,
        &PathBuf::from(format!("{}/{}", dir_rel, IGNORE_DIR_PROBE_CHILD)),
    )
}

pub fn list_directory(
    paths: &[PathBuf],
    rel_base: &PathBuf,
    filter: Option<TraversalFilter<'_>>,
) -> Result<ListDirectories, DfmError> {
    trace!("list directories with filter {:?}", filter);

    let mut error_messages = Vec::new();

    // Walk the tree and report progress periodically so large traversals
    // (e.g. `dfm add`/`dfm status` over $HOME) do not look frozen.
    // Progress is written straight to stderr (not via the `log` crate) so it
    // is shown at every verbosity level, and reuses a single line in place.
    // Pruned subtrees are never yielded by `filter_entry`, so skipped
    // directories do not count toward the visited-entry counter.
    const TRAVERSE_PROGRESS_STEP: usize = 500;
    let mut traversed_paths: Vec<PathBuf> = Vec::new();
    let mut pruned_dirs: Vec<String> = Vec::new();
    let mut visited = 0usize;
    let mut progress = ProgressLine::new();

    for path in paths.iter() {
        let keep_entry = |dir_entry: &DirEntry| -> bool {
            // Always keep the walk root: pruning it would silently drop an
            // explicitly named path (e.g. `dfm add some_ignored_dir`).
            if dir_entry.depth() == 0 {
                return true;
            }

            let rel = match dir_entry.path().strip_prefix(rel_base) {
                Ok(r) => r,
                Err(_) => match dir_entry.path().strip_prefix(path) {
                    Ok(r) => r,
                    Err(_) => return true,
                },
            };
            let Some(rel_str) = rel.to_str() else { return true };

            match filter {
                None => true,
                Some(TraversalFilter::KeepMatching(regex)) => regex.is_match(rel_str),
                Some(TraversalFilter::PruneIgnoredDirs(regex)) => {
                    if dir_entry.file_type().is_dir() && is_dir_ignored(regex, rel_str) {
                        pruned_dirs.push(rel_str.to_string());
                        false
                    } else {
                        true
                    }
                }
            }
        };

        for entry in WalkDir::new(path)
            .follow_links(false)
            .follow_root_links(false) // do not traverse symlinks pointing to dirs
            .into_iter()
            .filter_entry(keep_entry)
        {
            visited += 1;
            if visited % TRAVERSE_PROGRESS_STEP == 0 {
                progress.set(&format!("traversing... {} entries visited", visited));
            }
            match entry {
                Ok(d) if !d.file_type().is_dir() => traversed_paths.push(d.path().to_path_buf()),
                Err(ref e) => match e.io_error() {
                    Some(err) if err.kind() == io::ErrorKind::NotFound => {
                        traversed_paths.push(path.into());
                    },
                    _ => {
                        error_messages.push(format!("error: {}", e));
                    },
                },
                // we don't manage directories in source directory
                _ => {}
            }
        }
    }

    traversed_paths.dedup();
    pruned_dirs.dedup();

    Ok(ListDirectories {
        found: traversed_paths,
        errors: error_messages,
        pruned: pruned_dirs,
    })
}

#[derive(Eq, PartialEq)]
pub enum CompareByTimestamp {
    TargetModified,
    SourceModified,
    BothModified,
    NonModified,
    NeverSynchronized,
}

pub fn compare_files_by_timestamps(target_abs_path: &PathBuf, source_abs_path: &PathBuf, sync_time_opt: Option<&SystemTime>) -> Result<CompareByTimestamp, DfmError> {
    let target_file_meta = match target_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => return Err(DfmError::Io(e)),
    };

    let source_file_meta = match source_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => return Err(DfmError::Io(e)),
    };

    let source_file_synced = match sync_time_opt {
        Some(t) => *t,
        None => {
            debug!("synchronization time is not available for target {:?}\n\tand source {:?}",
                target_abs_path, source_abs_path);
            return Ok(CompareByTimestamp::NeverSynchronized);
        }
    };
    let target_file_modified = target_file_meta.modified().unwrap();
    let source_file_modified = source_file_meta.modified().unwrap();

    debug!("current state:\n target: mtime={:?}\n source: sync={:?},\n         mtime={:?}",
             target_file_modified, source_file_synced, source_file_modified);

    let both_not_modified = target_file_modified == source_file_synced &&
        source_file_synced == source_file_modified;
    let only_source_modified = target_file_modified == source_file_synced &&
        source_file_synced < source_file_modified || target_file_modified < source_file_modified;
    let only_target_modified = target_file_modified > source_file_synced &&
        source_file_synced == source_file_modified || target_file_modified > source_file_modified;
    let both_modified = target_file_modified > source_file_synced &&
        source_file_synced < source_file_modified;

    // TODO if source file does not required to be changed still
    //  need to check its permissions, and copy them if needed.
    //  Modifying permission does not make modification date change.

    // conflict cases
    if both_modified {
        return Ok(CompareByTimestamp::BothModified);
    }

    if only_source_modified {
        return Ok(CompareByTimestamp::SourceModified);
    }

    if both_not_modified {
        return Ok(CompareByTimestamp::NonModified);
    }

    if only_target_modified {
        return Ok(CompareByTimestamp::TargetModified);
    }

    Err(DfmError::other("the timestamps of the files under comparison are in inconsistent state"))
}

/// SHA-256 of the file contents, hex-encoded.
pub fn compute_sha256(path: &PathBuf) -> Result<String, DfmError> {
    use sha2::{Digest, Sha256};
    let content = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Content-based conflict detection for plain files. A side is "modified"
/// iff its current content hash differs from the hash stored at the last sync.
/// No mtime comparison is involved, so the result does not depend on
/// filesystem timestamp granularity.
pub fn compare_files_by_content(
    target_abs_path: &PathBuf,
    source_abs_path: &PathBuf,
    sync_time_opt: Option<&SyncTime>,
) -> Result<CompareByTimestamp, DfmError> {
    let sync = match sync_time_opt {
        Some(s) => s,
        None => return Ok(CompareByTimestamp::NeverSynchronized),
    };

    let target_modified = compute_sha256(target_abs_path)? != sync.sha256;
    let source_modified = compute_sha256(source_abs_path)? != sync.sha256;

    match (target_modified, source_modified) {
        (true, true) => Ok(CompareByTimestamp::BothModified),
        (true, false) => Ok(CompareByTimestamp::TargetModified),
        (false, true) => Ok(CompareByTimestamp::SourceModified),
        (false, false) => Ok(CompareByTimestamp::NonModified),
    }
}

/// Conflict detection that dispatches on the source file type: encrypted
/// sources (`.encrypted`) are compared by mtime (re-encryption produces
/// different bytes, so hashing is meaningless), plain files by content.
pub fn compare_files(
    encrypted_postfix: &str,
    target_abs_path: &PathBuf,
    source_abs_path: &PathBuf,
    sync_time_opt: Option<&SyncTime>,
) -> Result<CompareByTimestamp, DfmError> {
    if source_abs_path.as_os_str().to_string_lossy().ends_with(encrypted_postfix) {
        compare_files_by_timestamps(target_abs_path, source_abs_path, sync_time_opt.map(|s| &s.mtime))
    } else {
        compare_files_by_content(target_abs_path, source_abs_path, sync_time_opt)
    }
}

pub fn read_property_from_config(path_to_config_file: &PathBuf, param_name: &str) -> Result<Option<String>, DfmError> {
    let config_file_content = fs::read_to_string(path_to_config_file)?;
    let config: Table = toml::from_str(&config_file_content)?;
    return match config.get(param_name) {
        Some(v) => {
            Ok(Some(v.to_string()))
        },
        None => Ok(None)
    };
}

pub fn write_property_to_config(path_to_config_file: &PathBuf, param_name: &str, param_new_value: &str) -> Result<(), DfmError> {
    let config_file_content = fs::read_to_string(path_to_config_file)?;
    let mut config: Table = toml::from_str(&config_file_content)?;
    config.insert(param_name.to_owned(), Value::String(param_new_value.to_owned()));
    let new_content = toml::to_string_pretty(&config)?;
    fs::write(path_to_config_file, new_content)?;
    Ok(())
}

pub fn read_properties_from_config(path_to_config_file: &PathBuf) -> Result<Vec<String>, DfmError> {
    let config_file_content = fs::read_to_string(path_to_config_file)?;
    let config: Table = toml::from_str(&config_file_content)?;
    let mut params = vec![];
    for (_, (name, value)) in config.iter().enumerate() {
        params.push(format!("{} = {}", name, value));
    }
    Ok(params)
}

#[test]
fn test_file_path_relative_to() {
    assert_eq!(file_path_relative_to(&PathBuf::from("/a/b/c/d"), &PathBuf::from("/a/b/c")), PathBuf::from("d"));
    assert_eq!(file_path_relative_to(&PathBuf::from("/a/b/c/d"), &PathBuf::from("/a/b/c/")), PathBuf::from("d"));
    assert_eq!(file_path_relative_to(&PathBuf::from("/a/b/c/d"), &PathBuf::from("/a/b/c/d")), PathBuf::from("."));
}

#[test]
fn test_remove_dots_from_path() {
    assert_eq!(remove_dots_from_path(&PathBuf::from("/")), PathBuf::from("/"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a")), PathBuf::from("/a"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a/e")), PathBuf::from("/a/e"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a/b/e/..")), PathBuf::from("/a/b"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a/b/c/../../d/e")), PathBuf::from("/a/d/e"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a/b/../../d/e")), PathBuf::from("/d/e"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a/../../d/e")), PathBuf::from("../d/e"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/../../d/e")), PathBuf::from("../../d/e"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a/b/e/./f/g")), PathBuf::from("/a/b/e/f/g"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("./f/g")), PathBuf::from("f/g"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("f/g")), PathBuf::from("f/g"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("f/../g")), PathBuf::from("g"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("f/../")), PathBuf::from("."));
    assert_eq!(remove_dots_from_path(&PathBuf::from("./")), PathBuf::from("."));
    assert_eq!(remove_dots_from_path(&PathBuf::from(".")), PathBuf::from("."));
    assert_eq!(remove_dots_from_path(&PathBuf::from("..")), PathBuf::from(".."));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a/..")), PathBuf::from("/"));
    assert_eq!(remove_dots_from_path(&PathBuf::from("/a/../../b")), PathBuf::from("../b"));
}

#[test]
fn test_pattern_matches_path_components_single_component() {
    // Single sub-pattern, exact match — root level always matches
    assert!(pattern_matches_path_components(r"abc\.c", "abc.c"));
    assert!(pattern_matches_path_components(r"abc\.c", "./abc.c"));
    // No left glob — matches a directory component but NOT the last (file) component
    assert!(!pattern_matches_path_components(r"abc\.c", "dir/abc.c"));
    assert!(pattern_matches_path_components(r"abc\.c", "abc.c/file"));
    // Exact pattern does NOT match a different component
    assert!(!pattern_matches_path_components(r"abc\.c", "the-abc.c"));
    assert!(!pattern_matches_path_components(r"abc\.c", "dir/the-abc.c"));
    // With a left glob it matches at any depth
    assert!(pattern_matches_path_components(r".*abc\.c", "dir/abc.c"));
}

#[test]
fn test_pattern_matches_path_components_wildcard() {
    // Wildcard pattern .*abc\.c — matches any component ending with "abc.c"
    let pat = r".*abc\.c";
    assert!(pattern_matches_path_components(pat, "abc.c"));
    assert!(pattern_matches_path_components(pat, "the-abc.c"));
    assert!(pattern_matches_path_components(pat, "dir/the-abc.c"));
    assert!(pattern_matches_path_components(pat, "dir/abc.c"));
    assert!(pattern_matches_path_components(pat, "dir/abc.c/other"));
    // Does NOT match a component without the suffix
    assert!(!pattern_matches_path_components(pat, "abc.txt"));
}

#[test]
fn test_pattern_matches_path_components_cross_component_match() {
    let pat = r".*abc/def.*";
    // Adjacent components match
    assert!(pattern_matches_path_components(pat, "1abc/define"));
    assert!(pattern_matches_path_components(pat, "1abc/define/123"));
    assert!(pattern_matches_path_components(pat, "abc/define"));
    assert!(pattern_matches_path_components(pat, "hola/1abc/define/123"));
    // Non-adjacent: "abc" and "define" separated by "something"
    assert!(!pattern_matches_path_components(pat, "abc/something/define"));
}

#[test]
fn test_pattern_matches_path_components_cross_component_exact() {
    let pat = r"abc/def";
    assert!(pattern_matches_path_components(pat, "abc/def"));
    assert!(pattern_matches_path_components(pat, "abc/def/other"));
    assert!(pattern_matches_path_components(pat, "dir/abc/def"));
    assert!(!pattern_matches_path_components(pat, "abc/defx"));
    assert!(!pattern_matches_path_components(pat, "xabc/def"));
}

#[test]
fn test_pattern_does_not_match_subpath_components() {
    let pat = r"file\.txt";
    assert!(pattern_matches_path_components(pat, "./file.txt"));
    assert!(pattern_matches_path_components(pat, "file.txt"));
    assert!(!pattern_matches_path_components(pat, "abc/file.txt"));
}

#[test]
fn test_pattern_matches_path_components_empty_pattern() {
    assert!(!pattern_matches_path_components("", "anything"));
}

#[test]
fn test_pattern_matches_path_components_too_many_subpatterns() {
    // 3 sub-patterns but only 2 components → no match
    assert!(!pattern_matches_path_components(r"a/b/c", "a/b"));
}

#[test]
fn test_pattern_matches_path_components_trailing_slash_pattern() {
    // Trailing '/' must be treated the same as no slash: "dirname/" ignores
    // the dirname directory and everything inside it.
    assert!(pattern_matches_path_components("dirname/", "dirname/a.txt"));
    assert!(pattern_matches_path_components("dirname/", "a/dirname/b.txt"));
    assert!(pattern_matches_path_components("dirname/", "dirname"));
    assert!(!pattern_matches_path_components("dirname/", "other.txt"));
    assert!(!pattern_matches_path_components("dirname/", "the-dirname"));
    assert!(pattern_matches_path_components("a/b/", "x/a/b/f"));
    assert!(pattern_matches_path_components("a/b/", "/a/b"));
    assert!(pattern_matches_path_components("a/b/", "a/b"));
}

#[test]
fn test_pattern_matches_path_components_trailing_slash_equivalence() {
    // With and without trailing '/' must behave identically.
    let no_slash = "dirname";
    let with_slash = "dirname/";
    for rel in ["dirname/a.txt", "a/dirname/b.txt", "dirname", "other.txt", "x/dirname/y"] {
        assert_eq!(
            pattern_matches_path_components(no_slash, rel),
            pattern_matches_path_components(with_slash, rel),
            "trailing slash changed matching for {:?}",
            rel
        );
    }
}

#[test]
fn test_pattern_matches_path_components_trailing_slash_path() {
    // A relative path with a trailing '/' is normalized the same way.
    assert!(pattern_matches_path_components("dirname", "dirname/"));
    // A bare no-left-glob pattern does not match the final (file) position,
    // so "a/dirname/" (dirname as last element) does not match — same as
    // the no-slash form.
    assert!(!pattern_matches_path_components("dirname/", "a/dirname/"));
    assert!(!pattern_matches_path_components("abc\\.c", "the-abc.c/"));
}
