#[cfg(test)]
mod main_tests {

    use std::{fs, path::PathBuf, str::FromStr};
    use dfm::*;
    use crate::{Args, Command, init_command};
    use sealed_test::prelude::*;

    #[sealed_test]
    fn run_init_command_creates_files_and_directories_with_default_config() {
        let source_dir_path = PathBuf::from_str("dotfiles").unwrap(); // /tmp/.tmpQF1kAp/dotfiles
        fs::create_dir(&source_dir_path).unwrap();

        let args = Args {
            command: Command::Init {
                path_to_source: source_dir_path.clone(),
                path_to_target: Some(PathBuf::from_str(".").unwrap()),
                dry_run: false
            },
            dry_run: false,
            verbosity: 0,
            config: None
        };

        let config = create_default_config();

        match init_command(&args, &config) {
            Err(e) => panic!("init subcommand {:?}", e),
            Ok(_) => ()
        }

        assert_eq!(source_dir_path.exists(), true);
        assert_eq!(calc_config_file_path().unwrap().exists(), true); // /tmp/.tmp1GjBqs/.config/dfm/config.toml
        assert_eq!(calc_state_file_path().unwrap().exists(), true);  // /tmp/.tmpeQ5TUz/.local/state/dfm/state.toml
    }
}
