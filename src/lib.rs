pub mod crypt;

use std::collections::HashMap;
use std::{fs, io};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader};
use std::ops::Add;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

use envmnt::ExpandOptions;
use log::{debug, trace};
use log::error;
use microxdg::Xdg;
use regex::{Regex, RegexSet};
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use toml::{Table, Value};
use walkdir::{DirEntry, WalkDir};
use once_cell::sync::Lazy;
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

#[derive(Serialize, Deserialize, Clone)]
pub struct StateObject {
    pub source_directory: PathBuf,
    pub target_directory: PathBuf,
    pub syncs: HashMap<String, SystemTime>,
}

static STATE_DIRECTORY_NAME_IN_XDG_STATE: &str = "./dfm";
static STATE_FILE_NAME_IN_XDG_STATE: &str = "state.toml";

static CONFIG_FILE_NAME_IN_HOME: &str = ".dfm.toml";
static CONFIG_FILE_NAME_IN_XDG_CONFIG: &str = "config.toml";

static IGNORE_FILE_NAME_IN_XDG_STATE : &str = "ignore_file";
static IGNORE_FILE_NAME_IN_SOURCE_DIR: &str = "./.dfm_ignore_file";

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

static XDG : Lazy<Xdg> = Lazy::new(|| Xdg::new().expect("XDG directories must be available"));

pub fn calc_local_ignore_file() -> Result<PathBuf, DfmError> {
    let state_file_name = format!("{}/{}", STATE_DIRECTORY_NAME_IN_XDG_STATE, IGNORE_FILE_NAME_IN_XDG_STATE);
     return match XDG.state_file(&state_file_name) {
        Ok(p) => Ok(p),
        Err(e) => {
            error!("failed to find local ignore file: {}", e);
            return Err(DfmError::other(e));
        }
    };
}

pub fn open_or_create_target_ignore_file() -> Result<File, DfmError> {
    let state_file_name = format!("{}/{}", STATE_DIRECTORY_NAME_IN_XDG_STATE, IGNORE_FILE_NAME_IN_XDG_STATE);
    let p = match XDG.state_file(&state_file_name) {
        Ok(p) => p,
        Err(e) => {
            error!("failed to find local ignore file: {}", e);
            return Err(DfmError::other(e));
        }
    };
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
    let source_ignore_file_path = PathBuf::from_iter([source_dir_abs_path.to_str().unwrap(), &IGNORE_FILE_NAME_IN_SOURCE_DIR]);
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
    let haystack = haystack.to_str().unwrap();
    if regex.matches(haystack).matched_any() {
        let target_ignore_patterns = regex.patterns();
        for pattern in target_ignore_patterns {
            let regex = Regex::new(pattern).unwrap();
            if regex.is_match(haystack) {
                return Some(pattern.to_owned());
            }
        }
    }
    return None;
}

pub fn calc_state_directory_path() -> Result<PathBuf, DfmError> {
    match XDG.state() {
        Ok(path_to_config) => {
            Ok(PathBuf::from_iter([&path_to_config, &PathBuf::from(STATE_DIRECTORY_NAME_IN_XDG_STATE)]))
        },
        Err(e) => Err(DfmError::other(e))
    }
}

pub fn calc_state_file_path() -> Result<PathBuf, DfmError> {
    let state_file_name = format!("{}/{}", STATE_DIRECTORY_NAME_IN_XDG_STATE, STATE_FILE_NAME_IN_XDG_STATE);
    return match XDG.state_file(&state_file_name) {
        Ok(p) => Ok(p),
        Err(e) => {
            error!("failed to find state file: {}", e);
            return Err(DfmError::other(e));
        }
    };
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
    pub manage_symlinks: Option<bool>,
    // pub compare_content: Option<bool>, compare files by content

    // assign shell commands (with args of dfm) on the events of dfm
    // like: pre_add, post_add, on_add_failed, on_add_success
    // pre_add_merge, post_add_merge, on_add_merge_failed
    pub hooks: Option<Vec<HookFile>>,

    // if true ignore files and directories in the target directory
    // that don't start with a dot, by default - false
    pub dotfiles_only: Option<bool>,

    #[serde(with = "serde_regex")]
    pub force_encryption_for: Vec<Regex>,
    pub obtain_password_shell_command: Option<String>,
}

impl Config {
    pub fn from_settings(settings: &Settings) -> Self {
        Config {
            dot_prefix: Some(settings.dot_prefix.clone()),
            symlink_postfix: Some(settings.symlink_postfix.clone()),
            encrypted_postfix: Some(settings.encrypted_postfix.clone()),
            manage_symlinks: Some(settings.manage_symlinks),
            hooks: Some(settings.hooks.clone().into_iter().map(|h| HookFile {
                when: h.when,
                execute: h.execute
            }).collect()),
            dotfiles_only: Some(settings.dotfiles_only),
            force_encryption_for: settings.force_encryption_for.clone(),
            obtain_password_shell_command: settings.obtain_password_shell_command.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HookFile {
    pub when: String,
    pub execute: String,
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
    pub manage_symlinks: bool,
    // pub compare_content: Option<bool>, compare files by content

    // assign shell commands (with args of dfm) on the events of dfm
    // like: pre_add, post_add, on_add_failed, on_add_success
    // pre_add_merge, post_add_merge, on_add_merge_failed
    pub hooks:Vec<Hook>,

    // if true ignore files and directories in the target directory
    // that don't start with a dot, by default - false
    pub dotfiles_only: bool,

    // ignore `.dfm_root`, `.dfm_source_ignored`, `.dfm_target_ignored` by default
    // default_ignored: Vec<Regex>,
    pub force_encryption_for: Vec<Regex>,
    pub obtain_password_shell_command: Option<String>,

}

#[derive(Debug, Clone)]
pub struct Hook {
    pub when: String,
    pub execute: String,
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

pub fn calc_config_file_path() -> Result<PathBuf, DfmError>{
    let home_path = match get_home_path() {
        Some(p) => p,
        None => return Err(DfmError::Unsupported("Environment variable $HOME is not set".into()))
    };
    let config_in_home = PathBuf::from_iter(vec![home_path, PathBuf::from(CONFIG_FILE_NAME_IN_HOME)]);

    let path_to_config_file = match XDG.config() {
        Ok(path_to_config_dir) => {
            let state_file_name = format!("{}/{}", STATE_DIRECTORY_NAME_IN_XDG_STATE, CONFIG_FILE_NAME_IN_XDG_CONFIG);
            let config_path = PathBuf::from_iter(vec![path_to_config_dir.to_str().unwrap(), &state_file_name]);
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
        manage_symlinks: true,
        hooks: vec![],
        dotfiles_only: false,
        force_encryption_for: BY_DEFAULT_FORCE_ENCRYPTION_FILES.to_vec(),
        obtain_password_shell_command: Some("".to_owned()), // TODO need to make serde to add empy fiels to file
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
                    Some(state) => state.source_directory.to_str().unwrap().to_string(),
                    None => "".to_string()
                },
                target_dir: match state_object {
                    Some(state) => state.target_directory.to_str().unwrap().to_string(),
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
                manage_symlinks: match custom.manage_symlinks {
                    Some(v) => v,
                    None => default.manage_symlinks.to_owned()
                },
                hooks: vec![], // TODO implement hooks
                dotfiles_only: match custom.dotfiles_only {
                    Some(v) => v,
                    None => default.dotfiles_only.to_owned()
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

    if target_file_rel_to_target_dir_path_opt.is_some() {
        let ret = target_file_rel_to_target_dir_path_opt.unwrap();
        return if ret.to_str().unwrap().is_empty() { PathBuf::from(".") } else { ret };
    } else {
        let mut target_file_rel_to_target_dir_path_with_backs = String::from(file_abs_path.to_str().unwrap());
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
    let filename = regexp_for_leading_dot_in_filename.replace(target_file_rel_to_target_dir_path.file_name().unwrap().to_str().unwrap(), dot_prefix).to_string();
    let parent = regexp_for_leading_dot_in_filename.replace(target_file_rel_to_target_dir_path.parent().unwrap().to_str().unwrap(), dot_prefix).to_string();
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
    let ret = PathBuf::from_iter(vec![source_dir_abs_path.to_str().unwrap(), &source_file_rel_to_source_dir_path]);
    return remove_dots_from_path(&ret);
}

// TODO this is a shame, refactor this function with repentance
pub fn remove_dots_from_path(path: &PathBuf) -> PathBuf {
    if path.to_str().unwrap() == "/" {
        return PathBuf::from(path);
    }

    let mut go_back_counter = 0;
    let mut ret = String::new();
    for ancestor in path.iter().rev() {
        let name= ancestor.to_str().unwrap();
        let first_symbol_opt = ret.chars().nth(0);
        if name == "." && first_symbol_opt.is_some() && first_symbol_opt.unwrap() == '/' {
            ret.remove(0);
        } else if name == ".." {
            go_back_counter += 1;
        } else if name != "." && name != "/" {
            if go_back_counter > 0 {
                go_back_counter -=1;
            } else if !name.is_empty() {
                ret.insert_str(0, name);
                ret.insert(0, '/');
            }
        }
    }
    if !path.to_str().unwrap().starts_with("/") && ret.starts_with("/"){
        ret.remove(0);
    }
    if go_back_counter > 0 {
        ret.remove(0);
    }
    for _ in 0..go_back_counter {
        ret.insert_str(0, "../");
    }
    if ret == "" {
        ret.push('.');
    }
    return PathBuf::from(ret);
}

pub fn calc_working_dir_paths(settings: &Settings) -> Result<(PathBuf, PathBuf), DfmError> {
    if settings.source_dir.trim().is_empty() {
        error!("failed to read source directory path, does config file present on path {}?", "<todo>");
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
        Err(e) => {
            error!("source directory path is bad {}", e);
            return Err(DfmError::other(e));
        }
    };

    return Ok((target_dir_abs_path, source_dir_abs_path));
}

#[derive(Debug)]
pub struct ListDirectories {
    pub found: Vec<PathBuf>,
    pub errors: Vec<String>,
}

pub fn list_directory(paths: &[PathBuf], filter_regexp_opt: Option<&RegexSet>) -> Result<ListDirectories, DfmError> {
    trace!("list directories with filter {:?}", filter_regexp_opt);

    let ignore_filter =
        |dir_entry: &DirEntry| -> bool {
            return match dir_entry.path().to_str() {
                Some(p) => if let Some(regex) = filter_regexp_opt {
                    let matched = regex.is_match(p);
                    trace!("{} {}", p, if !matched { "❌" } else { "✔️" });
                    matched
                } else {
                    true
                },
                None => true
            };
        };

    let mut error_messages = Vec::new();
    let mut traversed_paths = paths.iter()
        .flat_map(|path| {
            WalkDir::new(path)
                .follow_links(false)
                .follow_root_links(false) // do not traverse symlinks pointing to dirs
                .into_iter()
                .filter_entry(ignore_filter)
                .map(|r| {
                    match r {
                        Ok(d) if !d.file_type().is_dir() => Some(d.path().to_path_buf()),
                        Err(ref e) if e.io_error()?.kind() == io::ErrorKind::NotFound => {
                            Some(path.into())
                        },
                        Err(ref e) => {
                            error_messages.push(format!("error: {}", e));
                            None
                        },
                        // we don't manage directories in source directory
                        _ => None
                    }
                })
                .filter(|o| o.is_some())
                .map(|o| o.unwrap())
                // FIXME do not create an array
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect::<Vec<PathBuf>>();

    traversed_paths.dedup();

    Ok(ListDirectories {
        found: traversed_paths,
        errors: error_messages,
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
        Err(e) => {
            error!("failed to read target {:?} metadata, {}", target_abs_path, e);
            return Err(DfmError::Io(e));
        }
    };

    let source_file_meta = match source_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => {
            error!("failed to read source {:?} metadata, {}", source_abs_path, e);
            return Err(DfmError::Io(e));
        }
    };

    let source_file_synced = match sync_time_opt {
        Some(t) => *t,
        None => {
            debug!("synchronization time is no available for target {:?}\n\tand source {:?}",
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
}
