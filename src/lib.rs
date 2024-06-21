mod crypt;

use std::collections::HashMap;
use std::fs;
use std::io::{Error, ErrorKind};
use std::ops::Add;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;

use log::trace;
use log::error;
use microxdg::Xdg;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Clone)]
pub struct StateObject {
    // TODO immediately, create new cool names for "source" and "target"!!!
    pub source_directory: PathBuf,
    pub target_directory: PathBuf,
    pub syncs: HashMap<String, SystemTime>,
}

static STATE_FILE_NAME_IN_XDG_STATE: &str = "./dfm/state.toml";

impl StateObject {
    pub fn new(target_directory: PathBuf, source_directory: PathBuf) -> Self {
       StateObject {
           source_directory,
           target_directory,
           syncs: HashMap::new()
       }
    }
}

pub fn calc_state_file_path() -> Result<PathBuf, Error> {
    let xdg = match Xdg::new() {
        Ok(v) => v,
        Err(e) => {
            error!("failed to obtain $XDG_CONFIG_PATH value: {}", e);
            return Err(Error::other(e));
        }
    };
    let path_to_state_file = match xdg.state_file(&STATE_FILE_NAME_IN_XDG_STATE) {
        Ok(p) => p,
        Err(e) => {
            error!("failed to find state file: {}", e);
            return Err(Error::other(e));
        }
    };
    Ok(path_to_state_file)
}

pub fn read_state(path_to_state_file: &PathBuf) -> Option<StateObject> {
    trace!("state file path {:?}", path_to_state_file);

    let state_file_content = match fs::read_to_string(path_to_state_file) {
        Ok(s) => s,
        Err(e) => {
            error!("failed to read state file {:?}: {}", path_to_state_file, e);
            return None;
        }
    };

    return match toml::from_str(&state_file_content) {
        Err(e) => {
            error!("failed to deserialize state file: {}", e);
            return None;
        },
        Ok(s) => Some(s)
    };
}

pub fn write_state(path_to_state_file: &PathBuf, state: &StateObject) -> Result<(), Error> {
    let state_content = match toml::to_string_pretty(state) {
        Ok(c) => c,
        Err(e) => {
            return Err(Error::new(ErrorKind::InvalidData, e));
        }
    };
    return fs::write(path_to_state_file, state_content);
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ConfigFile {
    pub dot_prefix: Option<String>,
    pub symlink_postfix: Option<String>,
    pub manage_symlinks: Option<bool>,
    // pub compare_content: Option<bool>, compare files by content

    // assign shell commands (with args of dfm) on the events of dfm
    // like: pre_add, post_add, on_add_failed, on_add_success
    // pre_add_merge, post_add_merge, on_add_merge_failed
    pub hooks: Option<Vec<HookFile>>,

    // if true ignore files and directories in the target directory
    // that don't start with a dot, by default - false
    pub dotfiles_only: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HookFile {
    pub when: String,
    pub execute: String,
}

// TODO rename to settings, rename ConfigFile to Config
#[derive(Debug, Clone)]
pub struct Config {
    pub config_file_found: bool,
    pub source_dir: String,
    pub target_dir: String,
    pub dot_prefix: String,
    pub symlink_postfix: String,
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
}

#[derive(Debug, Clone)]
pub struct Hook {
    pub when: String,
    pub execute: String,
}

static CONFIG_FILE_NAME_IN_HOME: &str = ".dfm.toml";
static CONFIG_FILE_NAME_IN_XDG_CONFIG: &str = "./dfm/config.toml";

pub fn write_config(path: &PathBuf, config: &ConfigFile) -> Result<(), Error> {
    let content = match toml::to_string_pretty(config) {
        Ok(c) => c,
        Err(e) => {
            return Err(Error::other(e));
        }
    };
    fs::write(path, content)
}

pub fn calc_config_file_path() -> Result<PathBuf, Error>{
    let xdg = match Xdg::new() {
        Ok(v) => v,
        Err(e) => {
            error!("failed to obtain $XDG_CONFIG_PATH value: {}", e);
            return Err(Error::other(e));
        }
    };

    if !envmnt::exists("HOME") { // TODO read HOME depending on operating system
        return Err(Error::new(ErrorKind::Unsupported, "Environment variable $HOME is not set"));
    }
    let home_path = envmnt::get_or_panic("HOME"); // TODO read HOME depending on operating system
    let config_in_home = PathBuf::from_iter(vec![home_path.as_str(), &CONFIG_FILE_NAME_IN_HOME]);

    let path_to_config_file = match xdg.config() {
        Ok(path_to_config_dir) => {
            let config_path = PathBuf::from_iter(vec![path_to_config_dir.to_str().unwrap(), &CONFIG_FILE_NAME_IN_XDG_CONFIG]);
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

pub fn create_default_config() -> Config {
    Config {
        config_file_found: false,
        source_dir: "".to_owned(),
        target_dir: "$HOME".to_owned(), // TODO read HOME depending on operating system
        dot_prefix: "dot_".to_owned(),
        symlink_postfix: ".symlink".to_owned(),
        manage_symlinks: true,
        hooks: vec![],
        dotfiles_only: false,
    }
}

pub fn read_config(path_to_config_file: &PathBuf) -> Option<ConfigFile> {
    trace!("config file path {:?}", path_to_config_file);

    let config_file_content = match fs::read_to_string(path_to_config_file) {
        Ok(s) => s,
        Err(e) => {
            error!("failed to read config file {:?}: {}", path_to_config_file, e);
            return None;
        }
    };

    return match toml::from_str(&config_file_content) {
        Err(e) => {
            error!("failed to deserialize state file: {}", e);
            return None;
        },
        Ok(c) => Some(c)
    };
}

pub fn merge_configs(default: &Config, custom_opt: &Option<ConfigFile>, state_object: &Option<StateObject>) -> Config {
    match custom_opt {
        Some(custom) =>
            Config {
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
                manage_symlinks: match custom.manage_symlinks {
                    Some(v) => v,
                    None => default.manage_symlinks.to_owned()
                },
                hooks: vec![], // TODO implement hooks
                dotfiles_only: match custom.dotfiles_only {
                    Some(v) => v,
                    None => default.dotfiles_only.to_owned()
                }
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

pub fn calc_working_dir_paths(config: &Config) -> Result<(PathBuf, PathBuf), Error> {
    if config.source_dir.trim().is_empty() {
        error!("failed to read source directory path, does config file present on path {}?", "<todo>");
        return Err(Error::other("failed to read source path from the config file: empty string"));
    }

    trace!("using target directory from config (original) {:?}", config.target_dir);

    let target_dir_path_expanded = envmnt::expand(&config.target_dir, None);
    trace!("using target directory from config (expanded) {}", target_dir_path_expanded);

    let target_dir_abs_path = match PathBuf::from_str(target_dir_path_expanded.as_str()) {
        Ok(p) => remove_dots_from_path(&p),
        Err(e) => return Err(Error::other(e))
    };

    trace!("using source directory from config (original) {:?}", config.source_dir);

    let source_dir_path_expanded = envmnt::expand(&config.source_dir, None);
    trace!("using source directory from config (expanded) {}", source_dir_path_expanded);

    let source_dir_abs_path = match PathBuf::from_str(source_dir_path_expanded.as_str()) {
        Ok(p) => remove_dots_from_path(&p),
        Err(e) => {
            error!("source directory path is bad {}", e);
            return Err(Error::other(e));
        }
    };

    return Ok((target_dir_abs_path, source_dir_abs_path));
}

#[derive(Debug)]
pub struct ListDirectories {
    pub found: Vec<PathBuf>,
    pub errors: Vec<String>,
}

pub fn list_directory(paths: &[PathBuf]) -> Result<ListDirectories, Error> {
    let mut error_messages = Vec::new();
    let mut traversed_paths = paths.iter()
        .flat_map(|path| {
            if path.is_dir() {
                WalkDir::new(path)
                    .follow_links(false)
                    .into_iter()
                    .map(|r| {
                        match r {
                            Ok(d) if !d.file_type().is_dir() => Some(d.path().to_path_buf()),
                            Err(e) => {
                                error_messages.push(format!("error: {}", e));
                                None
                            }
                            // we don't manage directories in source directory
                            _ => None
                        }
                    })
                    .filter(|o| o.is_some())
                    .map(|o| o.unwrap())
                    // FIXME do not create an array
                    .collect::<Vec<_>>()
                    .into_iter()
            } else {
                // FIXME do not create an array
                vec![path.clone()]
                    .into_iter()
            }
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

pub fn compare_files_by_timestamps(target_abs_path: &PathBuf, source_abs_path: &PathBuf, sync_time_opt: Option<&SystemTime>) -> Result<CompareByTimestamp, Error> {
    let target_file_meta = match target_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => {
            error!("failed to read target {:?} metadata, {}", target_abs_path, e);
            return Err(e);
        }
    };

    let source_file_meta = match source_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => {
            error!("failed to read source {:?} metadata, {}", source_abs_path, e);
            return Err(e);
        }
    };

    let source_file_synced = match sync_time_opt {
        Some(t) => *t,
        None => {
            trace!("synchronization time is no available for target {:?}\n\tand source {:?}",
                target_abs_path, source_abs_path);
            return Ok(CompareByTimestamp::NeverSynchronized);
        }
    };
    let target_file_modified = target_file_meta.modified().unwrap();
    let source_file_modified = source_file_meta.modified().unwrap();

    trace!("current state:\n target: mtime={:?}\n source: sync={:?},\n         mtime={:?}",
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

    Err(Error::other("the timestamps of the files under comparison are in inconsistent state"))
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
