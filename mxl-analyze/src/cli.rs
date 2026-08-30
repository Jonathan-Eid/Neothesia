use std::path::PathBuf;

use clap::{Command, arg, value_parser};

pub struct Args {
    pub input: PathBuf,
    pub out: Option<PathBuf>,
    pub no_llm: bool,
}

impl Args {
    pub fn get() -> Self {
        let matches = Command::new("mxl-analyze")
            .about("Per-note chord/scale analysis for a MusicXML file")
            .arg(
                arg!([INPUT])
                    .required(true)
                    .value_parser(value_parser!(PathBuf)),
            )
            .arg(
                arg!(--out <FILE>)
                    .required(false)
                    .value_parser(value_parser!(PathBuf)),
            )
            .arg(arg!(--"no-llm").required(false))
            .get_matches();

        Self {
            input: matches.get_one::<PathBuf>("INPUT").unwrap().clone(),
            out: matches.get_one::<PathBuf>("out").cloned(),
            no_llm: matches.get_flag("no-llm"),
        }
    }
}
