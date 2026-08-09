pub mod cli;
pub mod crypt;

use std::collections::HashMap;
use std::{fs, io};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;

use log::{debug, trace, warn};
use microxdg::Xdg;
use regex::{Regex, RegexSet};
use std::sync::LazyLock;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use toml::{Table, Value};
use walkdir::{DirEntry, WalkDir};

// Error type

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

    /// Returns `true` when the error is an I/O permission-denied error.
    pub fn is_permission_denied(&self) -> bool {
        matches!(self, DfmError::Io(e) if e.kind() == io::ErrorKind::PermissionDenied)
    }
}

/// Wrap an I/O error with the file path it occurred on, preserving the
/// `ErrorKind` (so `is_permission_denied()` still works) and the original
/// message. `std::io::Error`'s `Display` does not include the offending path,
/// so every user-facing file error is annotated here with the path.
pub fn io_err(path: &Path, e: io::Error) -> DfmError {
    DfmError::Io(io::Error::new(e.kind(), format!("{}: {}", path.display(), e)))
}

/// Same as `io_err` but associates two paths (a copy/source+destination pair).
pub fn io_copy_err(from: &Path, to: &Path, e: io::Error) -> DfmError {
    DfmError::Io(io::Error::new(
        e.kind(),
        format!("{} -> {}: {}", from.display(), to.display(), e),
    ))
}

/// Warn about an unreadable path (permission denied) instead of aborting the
/// command. Kept rule-consistent across all commands.
pub fn warn_unreadable(path: &Path, e: impl std::fmt::Display) {
    warn!("skipping unreadable path {:?}: {}", path, e);
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

// file name must be relative to target directory
static BY_DEFAULT_FORCE_ENCRYPTION_FILES: LazyLock<Vec<Regex>> = LazyLock::new(|| vec![Regex::from_str("\\.ssh").unwrap()]);

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
    open_or_create_file(&p)
}

pub fn open_or_create_file(path_to_file: &Path) -> Result<File, DfmError> {
    OpenOptions::new()
        .append(true)
        .create(true)
        .open(path_to_file)
        .map_err(|e| io_err(path_to_file, e))
}

pub fn calc_source_ignore_file(source_dir_abs_path: &Path) -> PathBuf {
    source_dir_abs_path.join(IGNORE_FILE_NAME_IN_SOURCE_DIR)
}

pub fn load_ignore_regex(ignore_file_path : &Path) -> Result<RegexSet, DfmError> {
    if !ignore_file_path.exists() {
        return Ok(RegexSet::empty());
    }

    let file = File::open(ignore_file_path).map_err(|e| io_err(ignore_file_path, e))?;
    let reader = BufReader::new(file);
    let mut patterns = vec![];

    for line in reader.lines() {
        let line = line.map_err(|e| io_err(ignore_file_path, e))?;
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

    if patterns.is_empty() {
        Ok(RegexSet::empty())
    } else {
        match RegexSet::new(patterns) {
            Ok(r) => Ok(r),
            Err(e) => Err(DfmError::other(e))
        }
    }
}

/// Full-path substring matcher: returns the first pattern in `regex` that
/// matches anywhere inside `haystack` (no anchoring).
///
/// This is the **substring** variant. It is intentionally used only for
/// `force_encryption_for` matching (see `add.rs`): a config value like
/// `\.ssh` is meant to match any path *containing* that text (e.g.
/// `/home/user/.ssh/config`), not just the exact final component. Everything
/// else (ignore files) uses `check_path_matches_regex_component_wise`, which
/// anchors per path component.
pub fn check_path_matches_regex_substring(regex: &RegexSet, haystack: &Path) -> Option<String> {
    let haystack = haystack.to_string_lossy();
    let matched_idx = regex.matches(haystack.as_ref()).iter().next()?;
    Some(regex.patterns()[matched_idx].to_owned())
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

/// Component-wise variant of `check_path_matches_regex_substring`: matches a
/// relative path between `/` separators instead of as a full-path substring.
///
/// `haystack` should be a path **relative** to the directory whose ignore
/// file is being checked (target or source).
pub fn check_path_matches_regex_component_wise(
    regex_set: &RegexSet,
    haystack: &Path,
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
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(DfmError::NotFound(format!(
                "state file does not exist: {}",
                path_to_state_file.display()
            )));
        }
        Err(e) => {
            return Err(io_err(path_to_state_file, e));
        }
    };

    let mut state: StateObject = match toml::from_str(&state_file_content) {
        Err(e) => {
            return Err(DfmError::InvalidData(format!(
                "state file is corrupt: {}: {}",
                path_to_state_file.display(),
                e
            )));
        },
        Ok(s) => s
    };

    state.syncs = state.syncs
        .into_iter()
        .map(|(key, sync)| {
            validate_state_key(&key)?;
            Ok((key, sync))
        })
        .collect::<Result<HashMap<_, _>, DfmError>>()?;

    Ok(state)
}

/// State keys are source-relative paths that get joined onto `source_dir` and
/// passed through lexical `..` resolution. Reject keys that could escape the
/// source directory (parent components, absolute paths, drive prefixes) when a
/// `state.toml` is tampered with. This is hardening, not an active exploit.
fn validate_state_key(key: &str) -> Result<(), DfmError> {
    let path = Path::new(key);
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(DfmError::InvalidData(format!(
                    "state key {:?} contains path component that escapes the source directory",
                    key
                )));
            }
        }
    }
    Ok(())
}

pub fn write_state(path_to_state_file: &PathBuf, state: &StateObject) -> Result<(), DfmError> {
    let state_content = toml::to_string_pretty(state)?;
    fs::write(path_to_state_file, state_content).map_err(|e| io_err(path_to_state_file, e))
}

/// Config read from the TOML file on disk (all fields optional).
#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub dot_prefix: Option<String>,
    pub symlink_postfix: Option<String>,
    pub encrypted_postfix: Option<String>,

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
    pub source_dir: String,
    pub target_dir: String,
    pub dot_prefix: String,
    pub symlink_postfix: String,
    pub encrypted_postfix: String,

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
    if let Some(config_parent_directory) = path_to_config_file.parent()
        && !config_parent_directory.exists()
    {
        fs::create_dir_all(config_parent_directory)
            .map_err(|e| io_err(config_parent_directory, e))?;
    }
    fs::write(path_to_config_file, content).map_err(|e| io_err(path_to_config_file, e))?;
    Ok(())
}

pub fn get_home_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
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

    Ok(path_to_config_file)
}

pub fn create_default_settings() -> Settings {
    Settings {
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
            return Err(io_err(path_to_config_file, e));
        }
    };

    match toml::from_str(&config_file_content) {
        Err(e) => Err(DfmError::other(format!(
            "config file corrupt: {}: {}",
            path_to_config_file.display(),
            e
        ))),
        Ok(c) => Ok(c)
    }
}

pub fn merge_settings(default: &Settings, custom_opt: &Option<Config>, state_object: Option<&StateObject>) -> Settings {
    // The source/target dirs are recorded in the state file (by `dfm init`) and
    // must be honored even when the config file is absent — otherwise removing
    // just the config (e.g. `rm -rf ~/.config/dfm`) breaks every
    // state-dependent command. The config file only supplies the tunable
    // fields; those fall back to defaults when it is missing.
    let source_dir = state_object
        .map(|s| s.source_directory.to_string_lossy().into_owned())
        .unwrap_or_default();
    let target_dir = state_object
        .map(|s| s.target_directory.to_string_lossy().into_owned())
        .unwrap_or_default();

    match custom_opt {
        Some(custom) => {
            Settings {
                source_dir,
                target_dir,
                dot_prefix: custom.dot_prefix.clone().unwrap_or_else(|| default.dot_prefix.clone()),
                symlink_postfix: custom.symlink_postfix.clone().unwrap_or_else(|| default.symlink_postfix.clone()),
                encrypted_postfix: custom.encrypted_postfix.clone().unwrap_or_else(|| default.encrypted_postfix.clone()),
                // `force_encryption_for` is a plain Vec, not an Option: an
                // explicitly empty list in config still means "use defaults".
                force_encryption_for: if custom.force_encryption_for.is_empty() {
                    default.force_encryption_for.clone()
                } else {
                    custom.force_encryption_for.clone()
                },
                obtain_password_shell_command: custom.obtain_password_shell_command.clone()
                    .or_else(|| default.obtain_password_shell_command.clone()),
                merge_tool_command: custom.merge_tool_command.clone()
                    .or_else(|| default.merge_tool_command.clone()),
            }
        }
        None => Settings {
            source_dir,
            target_dir,
            ..default.clone()
        },
    }
}

pub fn file_path_relative_to(file_abs_path: &Path, relative_to_abs_path: &Path) -> PathBuf {
    let mut target_file_rel_to_target_dir_path_opt: Option<PathBuf> = None;
    let mut path_components = Vec::new();
    for target_file_parent in file_abs_path.ancestors() {
        if relative_to_abs_path.eq(target_file_parent) {
            path_components.reverse();
            target_file_rel_to_target_dir_path_opt = Some(PathBuf::from_iter(path_components.iter().cloned()));
            break;
        }
        if let Some(filename) = target_file_parent.file_name() {
            path_components.push(filename.to_os_string());
        }
    }

    if let Some(ret) = target_file_rel_to_target_dir_path_opt {
        if ret.as_os_str().is_empty() { PathBuf::from(".") } else { ret }
    } else {
        let mut target_file_rel_to_target_dir_path_with_backs = file_abs_path.to_string_lossy().into_owned();
        for _ in 0..path_components.len() {
            target_file_rel_to_target_dir_path_with_backs.insert_str(0, "/..")
        }
        target_file_rel_to_target_dir_path_with_backs.insert(0, '.');
        PathBuf::from(target_file_rel_to_target_dir_path_with_backs)
    }
}

pub fn filepath_in_source_dir(dot_prefix: &str, target_dir_abs_path: &Path, source_dir_abs_path: &Path, target_abs_path: &Path, add_postfix_opt: Option<&str>) -> PathBuf {
    let target_file_rel_to_target_dir_path = file_path_relative_to(target_abs_path, target_dir_abs_path);

    trace!("target file path relative to target directory {:?}", target_file_rel_to_target_dir_path);

    // Encode the path into the source namespace by rewriting every leading-dot
    // component to `dot_prefix`: `.bashrc` -> `dot_bashrc`, and each dot
    // subdirectory likewise (`.config/foo` -> `dot_config/foo`). Iterating the
    // components maps all of them uniformly, so an encoding written by dfm
    // round-trips through `source_rel_to_target_rel` regardless of where a dot
    // appears in the path.
    //
    // The mapping must be injective: a target component that *literally* starts
    // with `dot_prefix` (e.g. `dot_backup`) would otherwise collide with the
    // encoding of `.backup`. Such components are escaped with a `~` marker
    // (`dot_backup` -> `~dot_backup`), and a literal leading `~` is doubled
    // (`~foo` -> `~~foo`), which `decode_source_rel_path` inverts.
    let mut source_rel = PathBuf::new();
    for component in target_file_rel_to_target_dir_path.components() {
        match component {
            Component::Normal(name) => {
                let name_str = name.to_string_lossy();
                if let Some(rest) = name_str.strip_prefix('.') {
                    source_rel.push(format!("{}{}", dot_prefix, rest));
                } else if name_str.starts_with(dot_prefix) || name_str.starts_with('~') {
                    source_rel.push(format!("~{}", name_str));
                } else {
                    source_rel.push(name);
                }
            }
            other => source_rel.push(other.as_os_str()),
        }
    }

    let mut source_file_rel_to_source_dir_path = source_rel;
    if let Some(postfix) = add_postfix_opt {
        let mut s = source_file_rel_to_source_dir_path.into_os_string();
        s.push(postfix);
        source_file_rel_to_source_dir_path = PathBuf::from(s);
    }

    trace!("source file path relative to source directory {:?}", source_file_rel_to_source_dir_path);
    let ret = source_dir_abs_path.join(&source_file_rel_to_source_dir_path);
    remove_dots_from_path(&ret)
}

/// Decode a source-relative path back into the target namespace, inverting
/// `filepath_in_source_dir` component by component (a plain `str::replace` is
/// asymmetric and corrupts components that merely contain the dot prefix, e.g.
/// `dot_config/dot_backup` -> `.config/.backup`).
///
/// A component starting with the dot prefix is the encoding of a hidden
/// component, a `~`-prefixed component is an escaped literal `dot_`-prefixed
/// name, and a `~~`-prefixed component is an escaped literal `~`. When
/// `hidden_as_dot` is set hidden components get their leading dot restored;
/// otherwise the prefix is dropped entirely (used for ignore-file
/// canonicalization, which stores the dotless form).
pub fn decode_source_rel_path(source_rel: &str, dot_prefix: &str, hidden_as_dot: bool) -> PathBuf {
    let mut target_rel = PathBuf::new();
    for component in Path::new(source_rel).components() {
        match component {
            Component::Normal(name) => {
                let name_str = name.to_string_lossy();
                if let Some(rest) = name_str.strip_prefix("~~") {
                    target_rel.push(format!("~{}", rest));
                } else if let Some(rest) = name_str.strip_prefix('~') {
                    target_rel.push(rest);
                } else if let Some(rest) = name_str.strip_prefix(dot_prefix) {
                    if hidden_as_dot {
                        target_rel.push(format!(".{}", rest));
                    } else {
                        target_rel.push(rest);
                    }
                } else {
                    target_rel.push(name);
                }
            }
            other => target_rel.push(other.as_os_str()),
        }
    }
    target_rel
}

/// Convert a source-relative path (state key) to a target-relative path,
/// stripping encrypted/symlink postfixes.
pub fn source_rel_to_target_rel(
    source_rel: &str,
    dot_prefix: &str,
    symlink_postfix: &str,
    encrypted_postfix: &str,
) -> String {
    let mut target_rel = decode_source_rel_path(source_rel, dot_prefix, true)
        .to_string_lossy()
        .into_owned();
    if target_rel.ends_with(symlink_postfix) {
        target_rel = target_rel[..target_rel.len() - symlink_postfix.len()].to_string();
    } else if target_rel.ends_with(encrypted_postfix) {
        target_rel = target_rel[..target_rel.len() - encrypted_postfix.len()].to_string();
    }
    target_rel
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
        return Err(DfmError::NotFound(
            "source directory is not set (state file missing or not initialized); run `dfm init`".into(),
        ));
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

    Ok((target_dir_abs_path, source_dir_abs_path))
}

// Single-line progress indicator

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
                Some(TraversalFilter::PruneIgnoredDirs(regex))
                    if dir_entry.file_type().is_dir() && is_dir_ignored(regex, rel_str) =>
                {
                    pruned_dirs.push(rel_str.to_string());
                    false
                }
                Some(TraversalFilter::PruneIgnoredDirs(_)) => true,
            }
        };

        for entry in WalkDir::new(path)
            .follow_links(false)
            .follow_root_links(false) // do not traverse symlinks pointing to dirs
            .into_iter()
            .filter_entry(keep_entry)
        {
            visited += 1;
            if visited.is_multiple_of(TRAVERSE_PROGRESS_STEP) {
                progress.set(&format!("traversing... {} entries visited", visited));
            }
            match entry {
                Ok(d) if !d.file_type().is_dir() => traversed_paths.push(d.path().to_path_buf()),
                Err(ref e) => match e.io_error() {
                    Some(err) if err.kind() == io::ErrorKind::NotFound => {
                        traversed_paths.push(path.into());
                    },
                    Some(err) if err.kind() == io::ErrorKind::PermissionDenied => {
                        // Unreadable objects (strict permissions/ownership) are
                        // skipped with a warning instead of aborting the command.
                        warn_unreadable(e.path().unwrap_or(path), err);
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

pub fn compare_files_by_timestamps(target_abs_path: &Path, source_abs_path: &Path, sync_time_opt: Option<&SystemTime>) -> Result<CompareByTimestamp, DfmError> {
    let target_file_meta = match target_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => return Err(io_err(target_abs_path, e)),
    };

    let source_file_meta = match source_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => return Err(io_err(source_abs_path, e)),
    };

    let source_file_synced = match sync_time_opt {
        Some(t) => *t,
        None => {
            debug!("synchronization time is not available for target {:?}\n\tand source {:?}",
                target_abs_path, source_abs_path);
            return Ok(CompareByTimestamp::NeverSynchronized);
        }
    };
    let target_file_modified = target_file_meta.modified().map_err(|e| io_err(target_abs_path, e))?;
    let source_file_modified = source_file_meta.modified().map_err(|e| io_err(source_abs_path, e))?;

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
pub fn compute_sha256(path: &Path) -> Result<String, DfmError> {
    use sha2::{Digest, Sha256};
    let file = fs::File::open(path).map_err(|e| io_err(path, e))?;
    let mut reader = BufReader::with_capacity(1 << 17, file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Content-based conflict detection for plain files. A side is "modified"
/// iff its current content hash differs from the hash stored at the last sync.
/// No mtime comparison is involved, so the result does not depend on
/// filesystem timestamp granularity.
pub fn compare_files_by_content(
    target_abs_path: &Path,
    source_abs_path: &Path,
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
    target_abs_path: &Path,
    source_abs_path: &Path,
    sync_time_opt: Option<&SyncTime>,
) -> Result<CompareByTimestamp, DfmError> {
    if source_abs_path.as_os_str().to_string_lossy().ends_with(encrypted_postfix) {
        compare_files_by_timestamps(target_abs_path, source_abs_path, sync_time_opt.map(|s| &s.mtime))
    } else {
        compare_files_by_content(target_abs_path, source_abs_path, sync_time_opt)
    }
}

pub fn read_property_from_config(path_to_config_file: &PathBuf, param_name: &str) -> Result<Option<String>, DfmError> {
    let config_file_content = fs::read_to_string(path_to_config_file).map_err(|e| io_err(path_to_config_file, e))?;
    let config: Table = toml::from_str(&config_file_content).map_err(|e| {
        DfmError::other(format!("config file corrupt: {}: {}", path_to_config_file.display(), e))
    })?;
    match config.get(param_name) {
        Some(v) => Ok(Some(v.to_string())),
        None => Ok(None)
    }
}

pub fn write_property_to_config(path_to_config_file: &PathBuf, param_name: &str, param_new_value: &str) -> Result<(), DfmError> {
    let config_file_content = fs::read_to_string(path_to_config_file).map_err(|e| io_err(path_to_config_file, e))?;
    let mut config: Table = toml::from_str(&config_file_content).map_err(|e| {
        DfmError::other(format!("config file corrupt: {}: {}", path_to_config_file.display(), e))
    })?;
    config.insert(param_name.to_owned(), Value::String(param_new_value.to_owned()));
    let new_content = toml::to_string_pretty(&config)?;
    fs::write(path_to_config_file, new_content).map_err(|e| io_err(path_to_config_file, e))?;
    Ok(())
}

pub fn read_properties_from_config(path_to_config_file: &PathBuf) -> Result<Vec<String>, DfmError> {
    let config_file_content = fs::read_to_string(path_to_config_file).map_err(|e| io_err(path_to_config_file, e))?;
    let config: Table = toml::from_str(&config_file_content).map_err(|e| {
        DfmError::other(format!("config file corrupt: {}: {}", path_to_config_file.display(), e))
    })?;
    let mut params = vec![];
    for (name, value) in config.iter() {
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
fn test_read_state_rejects_traversal_keys() {
    let dir = std::env::temp_dir().join(format!("dfm_state_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.toml");

    let good = r#"
source_directory = "/home/user/dotfiles"
target_directory = "/home/user"
[syncs]
"dot_bashrc" = { mtime = "0;0", sha256 = "" }
"dot_config/foo" = { mtime = "0;0", sha256 = "" }
"#;
    std::fs::write(&path, good).unwrap();
    let state = read_state(&path).unwrap();
    assert_eq!(state.syncs.len(), 2);

    let bad = r#"
source_directory = "/home/user/dotfiles"
target_directory = "/home/user"
[syncs]
"../../.bashrc" = { mtime = "0;0", sha256 = "" }
"#;
    std::fs::write(&path, bad).unwrap();
    assert!(read_state(&path).is_err());

    let bad_abs = r#"
source_directory = "/home/user/dotfiles"
target_directory = "/home/user"
[syncs]
"/etc/passwd" = { mtime = "0;0", sha256 = "" }
"#;
    std::fs::write(&path, bad_abs).unwrap();
    assert!(read_state(&path).is_err());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_merge_settings() {
    let default = create_default_settings();
    let state = StateObject::new(
        PathBuf::from("/home/user"),
        PathBuf::from("/home/user/dotfiles"),
    );

    // No config file -> the working dirs still come from the state file;
    // only the tunables fall back to defaults (A6).
    let no_custom = merge_settings(&default, &None, Some(&state));
    assert_eq!(no_custom.source_dir, "/home/user/dotfiles");
    assert_eq!(no_custom.target_dir, "/home/user");
    assert_eq!(no_custom.dot_prefix, default.dot_prefix);

    // Config present -> state supplies the working directories.
    let custom = Config {
        dot_prefix: Some("cfg_".to_string()),
        symlink_postfix: None,
        encrypted_postfix: None,
        force_encryption_for: vec![],
        obtain_password_shell_command: None,
        merge_tool_command: Some("meld {target} {source} {result}".to_string()),
    };
    let merged = merge_settings(&default, &Some(custom), Some(&state));
    assert_eq!(merged.source_dir, "/home/user/dotfiles");
    assert_eq!(merged.target_dir, "/home/user");
    // Config values win per field.
    assert_eq!(merged.dot_prefix, "cfg_");
    assert_eq!(merged.merge_tool_command, Some("meld {target} {source} {result}".to_string()));
    // Missing config fields fall back to defaults.
    assert_eq!(merged.symlink_postfix, default.symlink_postfix);
    assert_eq!(merged.encrypted_postfix, default.encrypted_postfix);
    assert_eq!(merged.obtain_password_shell_command, default.obtain_password_shell_command);
    // An explicitly empty force_encryption_for means "use defaults".
    assert_eq!(patterns(&merged.force_encryption_for), patterns(&default.force_encryption_for));

    // Non-empty force_encryption_for is taken from the config.
    let custom = Config {
        dot_prefix: None,
        symlink_postfix: None,
        encrypted_postfix: None,
        force_encryption_for: vec![Regex::new(r"\.ssh").unwrap()],
        obtain_password_shell_command: None,
        merge_tool_command: None,
    };
    let merged = merge_settings(&default, &Some(custom), None);
    assert_eq!(patterns(&merged.force_encryption_for), vec![r"\.ssh"]);
    // Without state the dirs stay empty even with a config.
    assert_eq!(merged.source_dir, "");
    assert_eq!(merged.target_dir, "");
}

#[cfg(test)]
fn patterns(regexes: &[Regex]) -> Vec<&str> {
    regexes.iter().map(Regex::as_str).collect()
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
