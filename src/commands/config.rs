use std::path::PathBuf;

use log::{debug, error};

use dfm::*;
use crate::{Args, Command, DfmError};

pub fn config_command(args: &Args, path_to_config_file: &PathBuf) -> Result<(), DfmError> {
    let Command::Config {
        get,
        set,
        list
    } = &args.command else {
        return Err(DfmError::Unsupported(format!("unreachable code reached: command {:?} is not `config`", args.command)));
    };

    match get {
        Some(param_name ) => {
            if let Ok(value_opt) = read_property_from_config(&path_to_config_file, param_name) {
                match value_opt {
                    Some(v) => {
                        println!("{}", v);
                    },
                    None => {
                        error!("parameter {} is not found", param_name);
                    }
                }
            } else {
                return Err(DfmError::other("config files does not exists"));
            };
        },
        None => {},
    }

    match set {
        Some(params) => {
            let param_name = params[0].clone();
            let param_new_vlue = params[1].clone();
            if args.dry_run {
                debug!("dry-run specified, nothing will be changed");
            } else if let Err(e) = write_property_to_config(&path_to_config_file, &param_name, &param_new_vlue) {
                return Err(DfmError::other(format!("failed to save config parameter value {:?}", e)));
            }
        },
        None => {}
    }

    if *list {
        match read_properties_from_config(&path_to_config_file) {
            Ok(props) => {
                for line in props {
                    println!("{}", line)
                }
            },
            Err(e) => {
                return Err(DfmError::other(format!("failed to read config {:?}", e)));
            },
        }
    }

    Ok(())
}
