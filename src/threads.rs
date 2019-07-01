#![allow(non_snake_case, dead_code)]
use {
    crate::{
        cli::ProgramArgs,
        match_with_log,
        models::{
            assets::{Output, OutputFormat, ReadFrom, Record},
            get_writer,
        },
    },
    csv::{ReaderBuilder, StringRecord},
    serde::{
        ser::SerializeSeq,
        {Serialize, Serializer},
    },
    serde_json::{map::Map as JMap, value::Value as JsonValue},
    serde_yaml::{Mapping as YMap, Value as YamlValue},
    std::{
        collections::BTreeSet,
        io::{BufWriter, Read as ioRead},
        sync::mpsc::{sync_channel as syncQueue, Receiver, SyncSender},
        thread::{spawn as thSpawn, JoinHandle},
    },
};

pub fn spawn_workers(opts: &'static ProgramArgs, from_source: Receiver<Box<dyn ioRead + Send>>) {
    // Reader
    let (ReBu_tx, ReBu_rx): (
        SyncSender<Receiver<(Vec<String>, Record)>>,
        Receiver<Receiver<(Vec<String>, Record)>>,
    ) = syncQueue(0);
    // Builder
    let (BuWr_tx, BuWr_rx): (SyncSender<Receiver<Output>>, Receiver<Receiver<Output>>) =
        syncQueue(0);
    let thReader = thSpawn(move || {
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
        // thBuilder.join()
    });

    // Builder
    let thBuilder = thSpawn(move || {
        let tx_writer = BuWr_tx;
        let rx_reader = ReBu_rx;
        let opts = &opts;

        while let Some(channel) = rx_reader.iter().next() {
            let (data_tx, data_rx): (SyncSender<Output>, Receiver<Output>) = syncQueue(10);
            tx_writer.send(data_rx).unwrap();
            channel
                .iter()
                .map(|(header, record)| match opts.output_type() {
                    OutputFormat::Json => Output::Json(build_json_item(header, record)),
                    OutputFormat::JsonPretty => Output::Json(build_json_item(header, record)),
                    OutputFormat::Yaml => Output::Yaml(build_yaml_item(header, record)),
                })
                .for_each(|item| {
                    data_tx.send(item).unwrap();
                })
        }

        // TODO: handle thread cleanup
        drop(tx_writer);
        // thWriter.join()
    });

    // Writer
    let thWriter = thSpawn(move || {
        let rx_builder = BuWr_rx;
        let opts = &opts;
        let mut writer = BufWriter::new(get_writer(opts.writer()));

        while let Some(channel) = rx_builder.iter().next() {
            let mut ser = serde_json::Serializer::new(&mut writer);
            let mut seq = ser.serialize_seq(None).unwrap();
            channel.iter().for_each(|output| {
                seq.serialize_element(&output).unwrap();
            });
            seq.end().unwrap();
        }
    });
}

pub fn build_json_item(hdr: Vec<String>, record: Record) -> JsonValue {
    let mut headers = hdr.iter().take(record.field_count as usize);
    let mut records = record.data.iter();
    let mut output = JMap::new();
    loop {
        let h_item = headers.next();
        let r_item = records.next();
        trace!("header: {:?}, field: {:?}", h_item, r_item);

        if h_item != None || r_item != None {
            let h_json = match h_item {
                Some(hdr) => hdr,
                None => "",
            };
            let r_json = match r_item {
                Some(rcd) => rcd,
                None => "",
            };
            output.insert(h_json.to_string(), JsonValue::String(r_json.to_string()));
        } else {
            break;
        }
    }
    trace!("Map contents: {:?}", &output);

    JsonValue::Object(output)
}

pub fn build_yaml_item(hdr: Vec<String>, record: Record) -> YamlValue {
    let mut headers = hdr.iter().take(record.field_count as usize);
    let mut records = record.data.iter();
    let mut output = YMap::new();
    loop {
        let h_item = headers.next();
        let r_item = records.next();
        trace!("header: {:?}, field: {:?}", h_item, r_item);

        if h_item != None || r_item != None {
            let h_json = match h_item {
                Some(hdr) => hdr,
                None => "",
            };
            let r_json = match r_item {
                Some(rcd) => rcd,
                None => "",
            };
            output.insert(
                YamlValue::String(h_json.to_string()),
                YamlValue::String(r_json.to_string()),
            );
        } else {
            break;
        }
    }
    trace!("Map contents: {:?}", &output);

    YamlValue::Mapping(output)
}

pub fn parse_csv_source<R>(
    opts: &ProgramArgs,
    source: R,
    tx_builder: SyncSender<(Vec<String>, Record)>,
) where
    R: ioRead,
{
    let mut rdr = ReaderBuilder::new()
        .delimiter(opts.delimiter())
        .flexible(opts.flexible())
        .escape(opts.escape())
        .comment(opts.comment())
        .quote(opts.quote())
        .trim(opts.trim_settings())
        .double_quote(opts.quote_settings().0)
        .quoting(opts.quote_settings().1)
        .from_reader(source);

    let mut headers: Headers = Headers::new(rdr.headers().unwrap());
    headers.extend(0);

    rdr.records()
        // Skip rows which error based on the CSV parser options, with a warning
        .filter_map(|result| match result {
            Ok(r) => Some(r),
            Err(e) => match_with_log!(None, warn!("Failed to parse record: {}, skipping...", e)),
        })
        // Parse CSV into a useable format and add metadata necessary for the conversion
        .map(|record| {
            record
                .iter()
                .map(|field| field.to_string())
                .scan(0u64, |count, record| {
                    *count += 1;
                    Some((*count, record))
                })
                .collect::<Record>()
        })
        .map(|wrapper| {
            let record_length = wrapper.field_count;
            if headers.length() < record_length {
                headers.extend(record_length)
            }

            (headers.list_copy(), wrapper)
        })
        .for_each(|(header, record)| {
            tx_builder.send((header, record)).unwrap(); // TODO: this will panic in the shutdown phase, fix it
        });
}

pub struct ThreadWrapper {
    pub reader: JoinHandle<()>,
    pub header: JoinHandle<()>,
    pub builder: JoinHandle<()>,
    pub writer: JoinHandle<()>,
}

#[derive(Clone)]
pub struct Headers {
    list: Vec<String>,
    length: usize,
}

impl Headers {
    pub fn new(unparsed_list: &StringRecord) -> Self {
        let list: Vec<String> = unparsed_list.iter().map(|csv| csv.to_string()).collect();
        let length = list.len();
        Headers { list, length }
    }

    pub fn length(&self) -> u64 {
        self.length as u64
    }

    pub fn list_copy(&self) -> Vec<String> {
        self.list.clone()
    }

    pub fn extend(&mut self, max_fields: u64) {
        let mut iter_binding_a;
        let mut iter_binding_b;
        let iter: &mut dyn Iterator<Item = (usize, String)> = match max_fields > self.length() {
            true => {
                let additional = (self.length() + 1..=max_fields)
                    .into_iter()
                    .map(|num| format!("__HEADER__{}", num));
                iter_binding_a = self.list.iter().cloned().chain(additional).enumerate();
                &mut iter_binding_a
            }
            false => {
                iter_binding_b = self.list.iter().cloned().enumerate();
                &mut iter_binding_b
            }
        };

        let extended = iter
            .scan(BTreeSet::new(), |dictionary, (index, header)| {
                if !dictionary.insert(header.clone()) {
                    let replacement = format!("__HEADER__{}", index);
                    let tail = match index {
                        i if i == 1 => format_args!("st"),
                        i if i == 2 => format_args!("nd"),
                        i if i == 3 => format_args!("rd"),
                        _ => format_args!("th"),
                    };
                    warn!(
                        "{}{} header is a duplicate! replacing [{}] with: [{}]",
                        index, tail, &header, replacement
                    );

                    dictionary.insert(replacement.clone());
                    Some(replacement)
                } else {
                    Some(header)
                }
            })
            .collect::<Vec<String>>();

        self.transmute(extended);
    }

    fn transmute(&mut self, replacement: Vec<String>) {
        let new_list = replacement;
        let new_length = new_list.len();

        std::mem::replace(&mut self.list, new_list);
        self.length = new_length;
    }
}
