use std::io::Error;
use std::ops::Add;
use std::path::PathBuf;
use std::str::FromStr;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub source_dir: String,
    pub target_dir: String,
    pub dot_prefix: Option<String>,
    pub manage_symlinks: Option<bool>,
    // pub compare_content: Option<bool>, compare files by content
    pub hooks: Option<Vec<Hook>>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Hook {
    pub when: String,
    pub execute: String,
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

pub fn filepath_in_source_dir(config: &Config, target_dir_abs_path: &PathBuf, source_dir_abs_path: &PathBuf, target_abs_path: &PathBuf, add_postfix_opt: Option<&str>) -> PathBuf {
    let regexp_for_leading_dot_in_filename = Regex::new(r#"^\."#).unwrap();
    let regexp_for_leading_dot_in_path = Regex::new(r#"/\.[^.]"#).unwrap();

    let dot_prefix = config.dot_prefix.clone().unwrap();
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

fn remove_dots_from_path(path: &PathBuf) -> PathBuf {
    if path.to_str().unwrap() == "/" {
        return PathBuf::from(path);
    }
 
    let mut go_back_counter = 0;
    let mut ret = String::new();
    for ancestor in path.iter().rev() {
        let name= ancestor.to_str().unwrap();
        if name == ".." {
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
}
