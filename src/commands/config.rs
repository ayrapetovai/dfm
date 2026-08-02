use std::path::PathBuf;

use log::{debug, warn};

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
            match read_property_from_config(&path_to_config_file, param_name) {
                Ok(Some(v)) => {
                    println!("{}", v);
                },
                Ok(None) => {
                    warn!("parameter {} is not found", param_name);
                },
                Err(e) => {
                    return Err(e);
                }
            }
        },
        None => {},
    }

    match set {
        Some(params) => {
            let param_name = params[0].clone();
            let param_new_value = params[1].clone();
            if args.dry_run {
                debug!("dry-run specified, nothing will be changed");
            } else {
                write_property_to_config(&path_to_config_file, &param_name, &param_new_value)?;
            }
        },
        None => {}
    }

    if *list {
        let props = read_properties_from_config(&path_to_config_file)?;
        for line in props {
            println!("{}", line)
        }
    }

    Ok(())
}
