use {
    crate::{
        cli::ProgramArgs,
        match_with_log,
        models::{
            assets::{Headers, ReadFrom, Record},
            error::ErrorKind,
        },
    },
    csv::ReaderBuilder,
    serde_json::{map::Map as JMap, value::Value as JsonValue},
    serde_yaml::{Mapping as YMap, Value as YamlValue},
    std::{
        boxed::Box,
        fs::{File, OpenOptions},
        io::{stdin as cin, stdout as cout, Read as ioRead, Write as ioWrite},
        path::PathBuf,
        sync::mpsc::SyncSender,
        vec::Vec,
    },
};

pub mod assets;
pub mod error;

// Determines write destination from runtime args
// w: (_, bool), true => append, false => create
pub fn get_writer(w: &(Option<String>, bool)) -> Box<dyn ioWrite> {
    match w {
        (Some(file_name), false) => match_with_log!(
            match File::create(file_name).ok() {
                Some(file) => match_with_log!(Box::new(file), info!("Success!")),
                None => match_with_log!(Box::new(cout()), warn!("Failed! Switching to stdout...")),
            },
            info!("Attempting to create {}...", file_name)
        ),
        (Some(file_name), true) => match_with_log!(
            match OpenOptions::new().append(true).open(file_name) {
                Ok(file) => match_with_log!(Box::new(file), info!("Success!")),
                Err(e) => match_with_log!(
                    Box::new(cout()),
                    warn!("Unable to open file: {}, switching to stdout...", e)
                ),
            },
            info!("Attempting to append to {}...", file_name)
        ),
        (None, _) => match_with_log!(
            Box::new(cout()),
            info!("No file detected, defaulting to stdout...")
        ),
    }
}

// Helper function for generating a list of read sources at runtime
pub fn get_reader(r: Option<&str>) -> Option<ReadFrom> {
    match r {
        Some("-") => Some(ReadFrom::Stdin),
        Some(file_name) => {
            let path = PathBuf::from(file_name);
            if path.is_file() {
                Some(ReadFrom::File(path))
            } else {
                None
            }
        }
        None => Some(ReadFrom::Stdin),
    }
}

// Opens a read source, defaults to stdin if source errors
pub fn set_reader(src: &Option<ReadFrom>) -> Box<dyn ioRead + Send> {
    match src {
        Some(s) => match s {
            ReadFrom::File(path) => match_with_log!(
                match File::open(path) {
                    Ok(f) => match_with_log!(Box::new(f), info!("Success!")),
                    Err(e) => match_with_log!(
                        Box::new(cin()),
                        warn!("Failed! {}, switching to stdin...", e)
                    ),
                },
                info!("Attempting to read from {:?}...", path)
            ),
            ReadFrom::Stdin => match_with_log!(Box::new(cin()), info!("Reading CSV from stdin...")),
        },
        None => match_with_log!(
            Box::new(cin()),
            info!("No input source found, defaulting to stdin...")
        ),
    }
}

// Parses CSV source into a manipulatable format
// that other functions can use to build JSON/YAML structures
pub(crate) fn parse_csv_source<R>(
    opts: &ProgramArgs,
    source: R,
    tx_builder: SyncSender<(Vec<String>, Record)>,
) -> Result<(), ErrorKind>
where
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

    // Hot loop
    let res = rdr
        .records()
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
        });
    for (header, record) in res {
        tx_builder.send((header, record)).map_err(|_| {
            ErrorKind::UnexpectedChannelClose(format!(
                "builder in |reader -> builder| channel has hung up"
            ))
        })?;
    }

    Ok(())
}

// Helper function for building Json compliant memory representations
pub fn build_json(hdr: Vec<String>, record: Record) -> JsonValue {
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

// Helper function for building Yaml compliant memory representations
pub fn build_yaml(hdr: Vec<String>, record: Record) -> YamlValue {
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
