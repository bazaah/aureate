#![allow(non_snake_case)]
use {
    crate::{
        cli::ProgramArgs,
        match_with_log,
        models::{
            assets::{Output, OutputFormat, Record},
            build_json, build_yaml, get_writer, parse_csv_source,
        },
    },
    serde::{ser::SerializeSeq, Serializer},
    std::{
        io::{BufWriter, Read as ioRead},
        sync::mpsc::{sync_channel as syncQueue, Receiver, SyncSender},
        thread::{spawn as thSpawn, JoinHandle},
    },
};

pub fn spawn_workers(
    opts: &'static ProgramArgs,
    from_source: Receiver<Box<dyn ioRead + Send>>,
) -> JoinHandle<()> {
    // Reader
    let (ReBu_tx, ReBu_rx): (
        SyncSender<Receiver<(Vec<String>, Record)>>,
        Receiver<Receiver<(Vec<String>, Record)>>,
    ) = syncQueue(0);
    // Builder
    let (BuWr_tx, BuWr_rx): (SyncSender<Receiver<Output>>, Receiver<Receiver<Output>>) =
        syncQueue(0);

    // Writer
    let thWriter = thSpawn(move || {
        debug!("Writer initialized");
        let rx_builder = BuWr_rx;
        let opts = &opts;
        let mut writer = BufWriter::new(get_writer(opts.writer()));
        info!("Buffered writer initialized");

        while let Some(channel) = rx_builder.iter().next() {
            match opts.output_type() {
                OutputFormat::Json => match_with_log!(
                    {
                        let mut ser = serde_json::Serializer::new(&mut writer);
                        let mut seq = ser.serialize_seq(None).unwrap();
                        channel.iter().for_each(|output| {
                            seq.serialize_element(&output).unwrap();
                        });
                        seq.end().unwrap();
                    },
                    info!("Using Json writer")
                ),
                OutputFormat::JsonPretty => match_with_log!(
                    {
                        let mut ser = serde_json::Serializer::pretty(&mut writer);
                        let mut seq = ser.serialize_seq(None).unwrap();
                        channel.iter().for_each(|output| {
                            seq.serialize_element(&output).unwrap();
                        });
                        seq.end().unwrap();
                    },
                    info!("Using pretty Json writer")
                ),
                OutputFormat::Yaml => match_with_log!(
                    {
                        let all_output: Vec<Output> = channel.iter().collect();
                        match serde_yaml::to_writer(&mut writer, &all_output) {
                            Ok(()) => (),
                            Err(e) => error!("Failed to write yaml: {}", e),
                        }
                    },
                    info!("Using Yaml writer")
                ),
            }
        }

        debug!("Writer closing");
    });

    // Builder
    let thBuilder = thSpawn(move || {
        debug!("Builder initialized");
        let tx_writer = BuWr_tx;
        let rx_reader = ReBu_rx;
        let opts = &opts;

        while let Some(channel) = rx_reader.iter().next() {
            let (data_tx, data_rx): (SyncSender<Output>, Receiver<Output>) = syncQueue(10);
            tx_writer.send(data_rx).unwrap();
            channel
                .iter()
                .map(|(header, record)| match opts.output_type() {
                    OutputFormat::Json => Output::Json(build_json(header, record)),
                    OutputFormat::JsonPretty => Output::Json(build_json(header, record)),
                    OutputFormat::Yaml => Output::Yaml(build_yaml(header, record)),
                })
                .for_each(|item| {
                    data_tx.send(item).unwrap();
                })
        }

        // TODO: handle thread cleanup
        drop(tx_writer);
        thWriter.join();
        debug!("Builder closing");
    });

    let thReader = thSpawn(move || {
        debug!("Reader initialized");
        let tx_builder = ReBu_tx;
        let opts = &opts;

        while let Some(src) = from_source.iter().next() {
            let (data_tx, data_rx): (
                SyncSender<(Vec<String>, Record)>,
                Receiver<(Vec<String>, Record)>,
            ) = syncQueue(10);
            tx_builder.send(data_rx).unwrap();
            parse_csv_source(&opts, src, data_tx);
        }

        // TODO: handle thread cleanup
        drop(tx_builder);
        thBuilder.join();
        debug!("Reader closing");
    });

    thReader
}
