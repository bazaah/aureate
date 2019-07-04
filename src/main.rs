#![feature(termination_trait_lib, try_trait)]
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;

use {
    crate::{
        cli::{generate_cli, ProgramArgs},
        models::{
            error::{ErrorKind, ProgramExit},
            set_reader,
        },
        threads::spawn_workers,
    },
    simplelog::*,
    std::{
        io::Read as ioRead,
        sync::mpsc::{sync_channel as syncQueue, Receiver, SyncSender},
    },
};

mod cli;
mod models;
mod threads;

lazy_static! {
    static ref CLI: ProgramArgs = ProgramArgs::init(generate_cli());
}

fn main() -> ProgramExit<ErrorKind> {
    // Start Pre-program code, do not place anything above these lines
    TermLogger::init(CLI.debug_level(), Config::default()).unwrap();
    info!("CLI options loaded and logger started");
    // End of Pre-program block

    let (tx, rx): (
        SyncSender<Box<dyn ioRead + Send>>,
        Receiver<Box<dyn ioRead + Send>>,
    ) = syncQueue(1);

    let reader = spawn_workers(&CLI, rx)?;
    for source in CLI.reader_list() {
        let read_from: Box<dyn ioRead + Send> = set_reader(source);
        tx.send(read_from).map_err(|_| {
            ErrorKind::UnexpectedChannelClose(format!(
                "reader in |main -> reader| channel has hung up"
            ))
        })?;
    }

    drop(tx);
    reader.join().map_err(|_| {
        ErrorKind::ThreadFailed(format!(
            "{}",
            std::thread::current().name().unwrap_or("unnamed")
        ))
    })??;

    ProgramExit::Success
}
