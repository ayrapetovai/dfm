use std::ops::Add;
use std::path::PathBuf;
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
            target_file_rel_to_target_dir_path_opt = Some(PathBuf::from_iter(path_components));
            break;
        }
        if let Some(filename) = target_file_parent.file_name() {
            path_components.insert(0, filename);
        }
    }

    // TODO if file does not belong to the given path, return
    //  something like "./../../../home/user/other/target/dir/file"
    //  when this relative path will be concatenated with the source directory path we'll get:
    //  /home/user/dotfiles/./../../../home/user/other/target/dir/file
    //  which will be resolved to /home/user/other/target/dir/file
    //  and added as /home/user/dotfiles/root_home/user/other/target/dir/file
    target_file_rel_to_target_dir_path_opt.unwrap()
}

pub fn filepath_in_source_dir(config: &Config, target_dir_abs_path: &PathBuf, source_dir_abs_path: &PathBuf, target_abs_path: &PathBuf, add_postfix_opt: Option<&str>) -> PathBuf {
    let regexp_for_leading_dot_in_filename = Regex::new(r"^\.").unwrap();
    let regexp_for_leading_dot_in_path = Regex::new(r"/\.").unwrap();

    let dot_prefix = config.dot_prefix.clone().unwrap();
    let slash_dot_prefix = String::from_iter(vec!["/", &dot_prefix]);

    let target_file_rel_to_target_dir_path = file_path_relative_to(&target_abs_path, &target_dir_abs_path);

    println!("target file path relative to target directory {:?}", target_file_rel_to_target_dir_path);

    // replace dots in filenames and dirnames to dot_prefix from config
    let source_file_rel_to_source_dir_path = regexp_for_leading_dot_in_filename
        .replace(target_file_rel_to_target_dir_path.to_str().unwrap(), &dot_prefix).to_string();
    let mut source_file_rel_to_source_dir_path = regexp_for_leading_dot_in_path
        .replace_all(&source_file_rel_to_source_dir_path, &slash_dot_prefix).to_string();

    if let Some(postfix) = add_postfix_opt {
        source_file_rel_to_source_dir_path = source_file_rel_to_source_dir_path.add(postfix);
    }

    println!("source file path relative to source directory {}", source_file_rel_to_source_dir_path);
    return PathBuf::from_iter(vec![source_dir_abs_path.to_str().unwrap(), &source_file_rel_to_source_dir_path]);
}
