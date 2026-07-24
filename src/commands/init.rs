use std::{env, fs};
use std::io::Write;
use std::path::PathBuf;

use log::{debug, error, info, trace, warn};

use dfm::*;
use crate::{Args, Command, DfmError};
use super::resolve_dry_run;

pub fn init_command(settings: &Settings, args: &Args) -> Result<(), DfmError> {
    let Command::Init {
        path_to_source,
        path_to_target: path_to_target_opt,
        dry_run,
        ..
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `init`", args.command)));
    };

    let dry_run = resolve_dry_run(*dry_run, args.dry_run);

    debug!("init with source path {:?}", path_to_source);
    debug!("init with target path {:?}", path_to_target_opt);

    enum InitTask {
        CreateSourceRootFile(PathBuf),
        CreateSourceIgnoreFile(),
        CreateStateFile(PathBuf, PathBuf, PathBuf),
        CreateDefaultConfigFile(PathBuf)
    }

    if !path_to_source.exists() {
        info!("source dir {:?} does not exist, creating", path_to_source);
        if dry_run {
            warn!("dry-run — skipping source directory creation");
        } else {
            let actual_path = if path_to_source.is_absolute() {
                path_to_source.clone()
            } else {
                let current_dir = env::current_dir()?;
                PathBuf::from_iter(vec![current_dir, path_to_source.clone()])
            };
            fs::create_dir_all(actual_path)?;
        }
    }

    let mut tasks = vec![];

    let mut source_directory_pointer = PathBuf::from_iter(vec![path_to_source.to_str().unwrap(), ".dfm_root"]);
    let source_dir_path = if source_directory_pointer.exists() {
        loop {
            let pointer_content = fs::read_to_string(&source_directory_pointer)?.trim().to_owned();
            if pointer_content == "." {
                break;
            } else {
                source_directory_pointer = PathBuf::from_iter(vec![source_directory_pointer.to_str().unwrap(), &pointer_content]);
            }
            trace!("searching .dfm_root in {:?}", source_directory_pointer);
        }
        fs::canonicalize(source_directory_pointer.parent().unwrap())?
    } else {
        tasks.push(InitTask::CreateSourceRootFile(PathBuf::from_iter(vec![path_to_source.to_str().unwrap(), ".dfm_root"])));
        fs::canonicalize(path_to_source)?
    };

    debug!("using source directory {:?}", source_dir_path);

    let source_ignore_file_path = calc_source_ignore_file(&source_dir_path)?;
    let source_ignore_regex = load_ignore_regex(&source_ignore_file_path)?;

    if !source_ignore_regex.matches(".dfm_root").matched_any() {
        debug!("source ignore file will be extended with \\.dfm_root");
        tasks.push(InitTask::CreateSourceIgnoreFile());
    }

    let home_dir_path = match get_home_path() {
        Some(p) => p,
        None => return Err(DfmError::InvalidData("failed to define home directory".into()))
    };

    let target_abs_path = if let Some(path_to_target) = path_to_target_opt {
        fs::canonicalize(path_to_target)?
    } else {
        home_dir_path
    };

    debug!("using target directory {:?}", target_abs_path);
    let state_file_path = calc_state_file_path()?;
    if state_file_path.exists() {
        debug!("state file already exists, no need to create");
    } else {
        tasks.push(InitTask::CreateStateFile(state_file_path.clone(), target_abs_path, source_dir_path.clone()));
    }

    let target_config_file_path = calc_config_file_path();
    if let Ok(config_file) = target_config_file_path {
        if !config_file.exists() {
            tasks.push(InitTask::CreateDefaultConfigFile(config_file));
        }
    }

    if dry_run {
        info!("dry run specified, no changes will be made");
    }

    if tasks.is_empty() {
        info!("nothing to do");
        return Ok(());
    }

    debug!("::init procedure begins, {} tasks", tasks.len());

    for task in tasks {
        match task {
            InitTask::CreateSourceRootFile(path) => {
                info!("create source root file {:?}", path);
                if dry_run {
                    continue;
                }
                fs::create_dir_all(path.parent().unwrap())?;
                fs::write(&path, ".")?;
            },
            InitTask::CreateSourceIgnoreFile() => {
                let mut ignore_file_records = vec![];
                ignore_file_records.push(".dfm_root");
                ignore_file_records.push(".git");
                ignore_file_records.push(".dfm_ignore_source");
                ignore_file_records.push(".dfm_ignore_target");

                info!("add file names to source ignore file {:?}", source_ignore_file_path);
                if dry_run {
                    continue;
                }

                fs::create_dir_all(source_ignore_file_path.parent().unwrap())?;
                let mut source_ignore_file = open_or_create_file(&source_ignore_file_path)?;

                for ignore_file_record in ignore_file_records {
                    if let Err(e) = writeln!(source_ignore_file, "{}", regex::escape(ignore_file_record)) {
                        error!("failed write path to file: {}", e);
                        return Err(e.into());
                    } else {
                        debug!("source ignore file: added record {}", ignore_file_record);
                    }
                }
            },
            InitTask::CreateStateFile(path, target_dir, source_dir) => {
                info!("create state file {:?}", path);
                if dry_run {
                    continue;
                }

                fs::create_dir_all(path.parent().unwrap())?;

                let empty_state = StateObject::new(target_dir, source_dir);
                write_state(&path, &empty_state)?;
            },
            InitTask::CreateDefaultConfigFile(path) => {
                info!("create config file {:?}", path);
                if dry_run {
                    continue;
                }

                let config_file = Config::from_settings(settings);
                write_config(&path, &config_file)?;
            }
        }
    }

    Ok(())
}
