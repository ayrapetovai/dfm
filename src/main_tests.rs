#[cfg(test)]
mod main_tests {

    use std::{env, fs, path::PathBuf, str::FromStr};
    use dfm::*;
    use crate::{Args, Command, add_command, init_command};
    use sealed_test::prelude::*;

    const SOURCE_DIR: &'static str = "./dotfiles";
    const TARGET_DIR: &'static str = "./.";
    const CONFIG_FILE: &'static str = ".config/dfm/config.toml";

    #[sealed_test]
    fn init_command_creates_files_and_directories_with_default_config() {
        let source_dir_path = PathBuf::from_str(SOURCE_DIR).unwrap(); // /tmp/.tmpQF1kAp/dotfiles
        let args = create_init_command_with_defaults();
        let config = create_default_test_config(None, None);

        let _ = init_command(&config, &args);

        assert_eq!(source_dir_path.exists(), true);
        assert_eq!(calc_config_file_path().unwrap().exists(), true); // /tmp/.tmp1GjBqs/.config/dfm/config.toml
        assert_eq!(calc_state_file_path().unwrap().exists(), true);  // /tmp/.tmpeQ5TUz/.local/state/dfm/state.toml
    }

    #[sealed_test]
    fn add_file_to_source() {
        let file_name = "file.txt";
        let target_file_path = PathBuf::from_iter(vec![TARGET_DIR, file_name]);

        let config_for_init = create_default_test_config(None, None);
        let init_args = create_init_command_with_defaults();
        let config_from_file = read_config_from_test_file();

        let mut state = create_empty_state();
        let config_for_add =  merge_configs(&config_for_init, &config_from_file, Some(&state));
        let add_args = create_add_command(Some(file_name));

        let _ = init_command(&config_for_init, &init_args);
        let _ = fs::write(target_file_path, "content");

        if let Err(e) = add_command(&config_for_add, &add_args, &mut state) {
            panic!("failed to add {:?}", e);
        }

        let source_file_path = PathBuf::from_iter(vec![SOURCE_DIR, file_name]);
        assert_eq!(source_file_path.exists(), true);
    }

    fn create_init_command_with_defaults() -> Args {
        let wd = if let Ok(p) = env::current_dir() {
            p.to_str().unwrap().to_owned()
        } else {
            panic!("faield to get working directory");
        };
        let source_dir_path = PathBuf::from_iter(vec![&wd, SOURCE_DIR]);
        if !source_dir_path.exists() {
            fs::create_dir(&source_dir_path).unwrap();
        }
        Args {
            command: Command::Init {
                path_to_source: source_dir_path.clone(),
                path_to_target: Some(PathBuf::from_str(TARGET_DIR).unwrap()),
                dry_run: false
            },
            dry_run: false,
            verbosity: 0,
            config: None
        }
    }

    fn create_default_test_config(source: Option<PathBuf>, target: Option<PathBuf>) -> Config {
        Config {
            config_file_found: true,
            source_dir: match source {
                Some(s) if s.exists() => s.to_str().unwrap().to_owned(),
                _ => PathBuf::from_iter(vec![env::current_dir().unwrap(), PathBuf::from_str(SOURCE_DIR).unwrap()]).to_str().unwrap().to_owned(),
            },
            target_dir: match target {
                Some(t) if t.exists() => t.to_str().unwrap().to_owned(),
                _ => PathBuf::from_iter(vec![env::current_dir().unwrap(), PathBuf::from_str(TARGET_DIR).unwrap()]).to_str().unwrap().to_owned(),
            },
            dot_prefix: "dot_".to_owned(),
            symlink_postfix: ".symlink".to_owned(),
            encrypted_postfix: ".encrypted".to_owned(),
            manage_symlinks: true,
            hooks: vec![],
            dotfiles_only: false,
            force_encryption_for: vec![],
            obtain_password_shell_command: Some("".to_owned())
        }
    }

    fn create_add_command(file_name: Option<&str>) -> Args {
        Args {
            command: Command::Add {
                paths: if let Some(f) = file_name {
                    Some(vec![PathBuf::from_str(f).unwrap()])
                } else {
                    None
                },
                merge: false,
                allow_foreign: false,
                force: false,
                symlink: false,
                encrypt: false,
                dry_run: false,
            },
            dry_run: false,
            verbosity: 0,
            config: None
        }
    }

    fn create_empty_state() -> StateObject {
        let path_to_state_file = match calc_state_file_path() {
            Ok(p) => p,
            Err(_) => panic!("failed to calc state file path")
        };
        return match read_state(&path_to_state_file) {
            Ok(s) => s,
            Err(_) => panic!("failed to read state")
        };
    }

    fn read_config_from_test_file() -> Option<ConfigFile> {
        let path_to_config_file = PathBuf::from_iter(vec![env::current_dir().unwrap().to_str().unwrap(), CONFIG_FILE]);
        return match read_config(&path_to_config_file) {
            Ok(c) => Some(c),
            Err(_) => None
        };
    }
}
