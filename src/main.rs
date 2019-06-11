#![feature(termination_trait_lib, try_trait)]
#[macro_use]
extern crate log;

use {
    crate::{
        cli::{generate_cli, ProgramArgs},
        models::{
            compose, csv_from_source, get_writer, outwriter, set_reader, ErrorKind, ProgramExit,
        },
    },
    simplelog::*,
    std::io::BufWriter,
};

mod cli;
mod models;

fn main() -> ProgramExit<ErrorKind> {
    // Start Pre-program code, do not place anything above these lines
    let clap = generate_cli();
    let cli = ProgramArgs::init(clap);
    TermLogger::init(cli.debug_level(), Config::default()).unwrap();
    info!("CLI options loaded and logger started");
    // End of Pre-program block

    let mut writer = BufWriter::new(get_writer(cli.writer()));
    info!("Buffered writer initialized");

    for source in cli.reader_list() {
        let parsed = csv_from_source(&cli, set_reader(source))?;

        let output = compose(&cli, parsed);
        let ot = cli.output_type().clone();
        outwriter(&mut writer, &output, &ot)?
    }

    ProgramExit::Success
}
