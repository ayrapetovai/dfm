use std::fs;
use std::io::Error;
use std::ops::Add;
use std::path::PathBuf;
use std::str::FromStr;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize, Clone)]
pub struct ConfigFile {
    pub source_dir: String,
    pub target_dir: Option<String>,
    pub dot_prefix: Option<String>,
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

#[derive(Debug, Clone)]
pub struct Config {
    pub config_file_found: bool,
    pub source_dir: String,
    pub target_dir: String,
    pub dot_prefix: String,
    pub manage_symlinks: bool,
    // pub compare_content: Option<bool>, compare files by content

    // assign shell commands (with args of dfm) on the events of dfm
    // like: pre_add, post_add, on_add_failed, on_add_success
    // pre_add_merge, post_add_merge, on_add_merge_failed
    pub hooks:Vec<Hook>,

    // if true ignore files and directories in the target directory
    // that don't start with a dot, by default - false
    pub dotfiles_only: bool,
}

#[derive(Debug, Clone)]
pub struct Hook {
    pub when: String,
    pub execute: String,
}

pub fn create_default_config() -> Config {
    Config {
        config_file_found: false,
        source_dir: "".to_owned(),
        target_dir: "$HOME".to_owned(),
        dot_prefix: "dot_".to_owned(),
        manage_symlinks: true,
        hooks: vec![],
        dotfiles_only: false,
    }
}

pub fn read_config(path_to_config_file: &PathBuf) -> Option<ConfigFile> {
    eprintln!("config file path {:?}", path_to_config_file);

    let config_file_content = match fs::read_to_string(path_to_config_file) {
        Ok(s) => s,
        Err(e) => {
            println!("failed to read config file {:?}: {}", path_to_config_file, e);
            return None
        }
    };

    return match toml::from_str(&config_file_content) {
        Err(_) => None,
        Ok(c) => Some(c)
    };
}

pub fn merge_configs(default: &Config, custom_opt: &Option<ConfigFile>) -> Config {
    match custom_opt {
        Some(custom) =>
            Config {
                config_file_found: true,
                source_dir: custom.source_dir.to_owned(),
                target_dir: match custom.target_dir.to_owned() {
                    Some(v) => v.clone(),
                    None => default.target_dir.to_owned()
                },
                dot_prefix: match &custom.dot_prefix {
                    Some(v) => v.clone(),
                    None => default.dot_prefix.to_string()
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
        target_file_rel_to_target_dir_path_opt.unwrap()
    } else {
        let mut target_file_rel_to_target_dir_path_with_backs = String::from(file_abs_path.to_str().unwrap());
        for _ in 0..path_components.len() {
            target_file_rel_to_target_dir_path_with_backs.insert_str(0, "/..")
        }
        target_file_rel_to_target_dir_path_with_backs.insert_str(0, ".");
        PathBuf::from(target_file_rel_to_target_dir_path_with_backs)
    }
}

// TODO replace `config` with dot_prefix: &String
pub fn filepath_in_source_dir(config: &Config, target_dir_abs_path: &PathBuf, source_dir_abs_path: &PathBuf, target_abs_path: &PathBuf, add_postfix_opt: Option<&str>) -> PathBuf {
    let regexp_for_leading_dot_in_filename = Regex::new(r#"^\."#).unwrap();
    let regexp_for_leading_dot_in_path = Regex::new(r#"/\.[^.]"#).unwrap();

    let dot_prefix = config.dot_prefix.clone();
    let slash_dot_prefix = String::from_iter(vec!["/", &dot_prefix]);

    let target_file_rel_to_target_dir_path = file_path_relative_to(target_abs_path, &target_dir_abs_path);

    println!("target file path relative to target directory {:?}", target_file_rel_to_target_dir_path);

    // replace dots in filenames and dirnames to dot_prefix from config
    let filename = regexp_for_leading_dot_in_filename.replace(target_file_rel_to_target_dir_path.file_name().unwrap().to_str().unwrap(), &dot_prefix).to_string();
    let parent = regexp_for_leading_dot_in_filename.replace(target_file_rel_to_target_dir_path.parent().unwrap().to_str().unwrap(), &dot_prefix).to_string();
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

    println!("source file path relative to source directory {}", source_file_rel_to_source_dir_path);
    let ret = PathBuf::from_iter(vec![source_dir_abs_path.to_str().unwrap(), &source_file_rel_to_source_dir_path]);
    return remove_dots_from_path(&ret);
}

pub fn remove_dots_from_path(path: &PathBuf) -> PathBuf {
    if path.to_str().unwrap() == "/" {
        return PathBuf::from(path);
    }

    let mut go_back_counter = 0;
    let mut ret = String::new();
    for ancestor in path.iter().rev() {
        let name= ancestor.to_str().unwrap();
        if name == "." && ret.chars().nth(0).unwrap() == '/' {
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
    if go_back_counter > 0 {
        ret.remove(0);
    }
    for _ in 0..go_back_counter {
        ret.insert_str(0, "../");
    }
    return PathBuf::from(ret);
}

pub fn calc_working_dir_paths(config: &Config) -> Result<(PathBuf, PathBuf), Error> {
    if config.source_dir.trim().is_empty() {
        println!("failed to read source directory path, does config file present on path {}?", "<todo>");
        return Err(Error::other("failed to read source path from the config file: empty string"));
    }

    println!("using target directory from config (original) {:?}", config.target_dir);

    let target_dir_path_expanded = envmnt::expand(&config.target_dir, None);
    println!("using target directory from config (expanded) {}", target_dir_path_expanded);

    let target_dir_abs_path = match PathBuf::from_str(target_dir_path_expanded.as_str()) {
        Ok(p) => remove_dots_from_path(&p),
        Err(e) => panic!("target directory path is bad {}", e)
    };

    println!("using source directory from config (original) {:?}", config.source_dir);

    let source_dir_path_expanded = envmnt::expand(&config.source_dir, None);
    println!("using source directory from config (expanded) {}", source_dir_path_expanded);

    let source_dir_abs_path = match PathBuf::from_str(source_dir_path_expanded.as_str()) {
        Ok(p) => remove_dots_from_path(&p),
        Err(e) => {
            println!("source directory path is bad {}", e);
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

pub fn list_directory(paths: &Vec<PathBuf>) -> Result<ListDirectories, Error> {
    let mut error_messages = Vec::new();
    let traversed_paths = paths.iter()
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
        // TODO filter duplicates
        .collect::<Vec<PathBuf>>();

    Ok(ListDirectories {
        found: traversed_paths,
        errors: error_messages,
    })
}

pub enum CompareByTimestamp {
    TargetModified,
    SourceModified,
    BothModified,
    NonModified,
}

pub fn compare_files_by_timestamps(target_abs_path: &PathBuf, source_file_abs_path: &PathBuf) -> Result<CompareByTimestamp, Error> {
    let target_file_meta = match target_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("failed to read target {:?} metadata, {}", target_abs_path, e);
            return Err(e);
        }
    };

    let source_file_meta = match source_file_abs_path.metadata() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("failed to read source {:?} metadata, {}", source_file_abs_path, e);
            return Err(e);
        }
    };

    let source_file_created = match source_file_meta.created() {
        Ok(t) => t,
        Err(e) => {
            println!("this filesystem does not support creation time for files (try to recompile the program): {}", e);
            return Err(e);
        }
    };
    let target_file_modified = target_file_meta.modified().unwrap();
    let source_file_modified = source_file_meta.modified().unwrap();

    // TODO if verbose
    println!("current state:\n target: mtime={:?}\n source: btime={:?},\n         mtime={:?}",
             target_file_modified, source_file_created, source_file_modified);

    let both_not_modified = target_file_modified == source_file_created &&
        source_file_created == source_file_modified;
    let only_source_modified = target_file_modified == source_file_created &&
        source_file_created < source_file_modified || target_file_modified < source_file_modified;
    let only_target_modified = target_file_modified > source_file_created &&
        source_file_created == source_file_modified || target_file_modified > source_file_modified;
    let both_modified = target_file_modified > source_file_created &&
        source_file_created < source_file_modified;

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

    if only_target_modified { // TODO if verbose
        return Ok(CompareByTimestamp::TargetModified);
    }

    Err(Error::other("the timestamps of the files under comparison are in inconsistent state"))
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
}
