#![allow(non_snake_case)]
use {
    crate::{
        cli::ProgramArgs,
        match_with_log,
        models::{
            assets::{Output, OutputFormat, Record},
            build_json, build_yaml,
            error::ErrorKind,
            get_writer, parse_csv_source,
        },
    },
    serde::{ser::SerializeSeq, Serializer},
    std::{
        io::{BufWriter, Read as ioRead},
        sync::mpsc::{sync_channel as syncQueue, Receiver, SyncSender},
        thread::{Builder as thBuilder, JoinHandle},
    },
};

pub(crate) fn spawn_workers(
    opts: &'static ProgramArgs,
    from_source: Receiver<Box<dyn ioRead + Send>>,
) -> Result<JoinHandle<Result<(), ErrorKind>>, ErrorKind> {
    // Meta channel: |Reader -> Builder|, delivers new receivers to builder
    let (ReBu_tx, ReBu_rx): (
        SyncSender<Receiver<(Vec<String>, Record)>>,
        Receiver<Receiver<(Vec<String>, Record)>>,
    ) = syncQueue(0);
    // Meta channel: |Builder -> Writer|, delivers new receivers to writer
    let (BuWr_tx, BuWr_rx): (SyncSender<Receiver<Output>>, Receiver<Receiver<Output>>) =
        syncQueue(0);

    // Writer
    let thWriter =
        thBuilder::new()
            .name(format!("Writer"))
            .spawn(move || -> Result<(), ErrorKind> {
                debug!("Writer initialized");
                let rx_builder = BuWr_rx;
                let opts = &opts;
                let mut writer = BufWriter::new(get_writer(opts.writer()));
                info!("Buffered writer initialized");

                // Hot loop
                while let Some(channel) = rx_builder.iter().next() {
                    let _res: Result<(), ErrorKind> = match opts.output_type() {
                        OutputFormat::Json => match_with_log!(
                            {
                                let mut ser = serde_json::Serializer::new(&mut writer);
                                let mut seq =
                                    ser.serialize_seq(None).map_err(|e| ErrorKind::from(e))?;
                                for output in channel.iter() {
                                    seq.serialize_element(&output)
                                        .map_err(|e| ErrorKind::from(e))?;
                                }
                                seq.end().map_err(|e| ErrorKind::from(e))?;
                                Ok(())
                            },
                            info!("Using Json writer")
                        ),
                        OutputFormat::JsonPretty => match_with_log!(
                            {
                                let mut ser = serde_json::Serializer::pretty(&mut writer);
                                let mut seq =
                                    ser.serialize_seq(None).map_err(|e| ErrorKind::from(e))?;
                                for output in channel.iter() {
                                    seq.serialize_element(&output)
                                        .map_err(|e| ErrorKind::from(e))?;
                                }
                                seq.end().map_err(|e| ErrorKind::from(e))?;
                                Ok(())
                            },
                            info!("Using pretty Json writer")
                        ),
                        OutputFormat::Yaml => match_with_log!(
                            {
                                let all_output: Vec<Output> = channel.iter().collect();
                                serde_yaml::to_writer(&mut writer, &all_output)
                                    .map_err(|e| ErrorKind::from(e))?;

                                Ok(())
                            },
                            info!("Using Yaml writer")
                        ),
                    };
                }

                // Cleanup
                debug!("Writer closing");
                Ok(())
            });

    // Builder
    let thBuilder =
        thBuilder::new()
            .name(format!("Builder"))
            .spawn(move || -> Result<(), ErrorKind> {
                debug!("Builder initialized");
                let tx_writer = BuWr_tx;
                let rx_reader = ReBu_rx;
                let opts = &opts;

                // Hot loop
                while let Some(channel) = rx_reader.iter().next() {
                    let (data_tx, data_rx): (SyncSender<Output>, Receiver<Output>) = syncQueue(10);
                    tx_writer.send(data_rx).map_err(|_| {
                        ErrorKind::UnexpectedChannelClose(format!(
                            "failed to send next |builder -> writer| channel, writer has hung up"
                        ))
                    })?;
                    let res = channel
                        .iter()
                        .map(|(header, record)| match opts.output_type() {
                            OutputFormat::Json => Output::Json(build_json(header, record)),
                            OutputFormat::JsonPretty => Output::Json(build_json(header, record)),
                            OutputFormat::Yaml => Output::Yaml(build_yaml(header, record)),
                        });
                    for item in res {
                        data_tx.send(item).map_err(|_| {
                            ErrorKind::UnexpectedChannelClose(format!(
                                "writer in |builder -> writer| channel has hung up"
                            ))
                        })?;
                    }
                }

                // Cleanup
                drop(tx_writer);
                thWriter?.join().map_err(|_| {
                    ErrorKind::ThreadFailed(format!(
                        "{}",
                        std::thread::current().name().unwrap_or("unnamed")
                    ))
                })??;
                debug!("Builder closing");
                Ok(())
            });

    // Reader
    let thReader: JoinHandle<Result<(), ErrorKind>> = thBuilder::new()
        .name(format!("Reader"))
        .spawn(move || -> Result<(), ErrorKind> {
            debug!("Reader initialized");
            let tx_builder = ReBu_tx;
            let opts = &opts;

            // Hot loop
            while let Some(src) = from_source.iter().next() {
                let (data_tx, data_rx): (
                    SyncSender<(Vec<String>, Record)>,
                    Receiver<(Vec<String>, Record)>,
                ) = syncQueue(10);
                tx_builder.send(data_rx).map_err(|_| {
                    ErrorKind::UnexpectedChannelClose(format!(
                        "failed to send next |reader -> builder| channel, builder has hung up"
                    ))
                })?;
                parse_csv_source(&opts, src, data_tx)?;
            }

            // Cleanup
            drop(tx_builder);
            thBuilder?.join().map_err(|_| {
                ErrorKind::ThreadFailed(format!(
                    "{}",
                    std::thread::current().name().unwrap_or("unnamed")
                ))
            })??;
            debug!("Reader closing");
            Ok(())
        })
        .map_err(|_| {
            ErrorKind::ThreadFailed(format!(
                "{}",
                std::thread::current().name().unwrap_or("unnamed")
            ))
        })?;

    Ok(thReader)
}
