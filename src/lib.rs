use std::path::PathBuf;

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

    // TODO if file does not belong to the given path, add
    //  something like "../../../home/user/other/target/dir/file"
    //  when this relative path will be concatenated with the source directory path we'll get:
    //  /home/user/dotfiles/../../../home/user/other/target/dir/file
    //  which will be resolved to /home/user/other/target/dir/file
    //  and added as /home/user/dotfiles/root_home/user/other/target/dir/file
    target_file_rel_to_target_dir_path_opt.unwrap()
}
