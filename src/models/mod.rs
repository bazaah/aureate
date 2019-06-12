use {
    crate::{
        cli::ProgramArgs,
        match_with_log,
        models::assets::{OutputFormat, ReadFrom, Record},
    },
    csv::ReaderBuilder,
    serde::Serialize,
    serde_json::{map::Map, value::Value as JVal},
    std::{
        boxed::Box,
        collections::BTreeSet,
        error::Error,
        fs::File,
        io::{stdin as cin, stdout as cout, Read as ioRead, Write as ioWrite},
        path::PathBuf,
        vec::Vec,
    },
};

pub mod assets;
pub mod error;

// Determines write destination from runtime args
pub fn get_writer(w: &Option<String>) -> Box<dyn ioWrite> {
    match w {
        Some(file_name) => match_with_log!(
            match File::create(file_name).ok() {
                Some(file) => match_with_log!(Box::new(file), info!("Success!")),
                None => match_with_log!(Box::new(cout()), warn!("Failed! Switching to stdout...")),
            },
            info!("Attempting to create {}...", file_name)
        ),
        None => match_with_log!(
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
pub fn set_reader(src: &Option<ReadFrom>) -> Box<dyn ioRead> {
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
pub fn csv_from_source<R>(
    opts: &ProgramArgs,
    source: R,
) -> Result<(Vec<String>, Vec<Record>), Box<dyn Error>>
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

    // Track maximum record row length
    let mut max_record_fields = 0u64;

    // Parse records
    let records = rdr
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
        .inspect(|wrapper| {
            if max_record_fields < wrapper.field_count {
                max_record_fields = wrapper.field_count;
            }
        })
        .collect::<Vec<Record>>();

    info!("Highest record field length: {}", max_record_fields);

    // Parse headers
    let csv_headers = rdr.headers()?;
    let hdr_fields = csv_headers.len();
    let max_fields = max_record_fields as usize;

    // Adds additional headers if any record row's length > header row length
    let mut iter_binding_a;
    let mut iter_binding_b;
    let iter: &mut dyn Iterator<Item = (usize, String)> = match max_fields > hdr_fields {
        true => {
            let additional = (hdr_fields + 1..=max_fields)
                .into_iter()
                .map(|num| format!("__HEADER__{}", num));
            iter_binding_a = csv_headers
                .iter()
                .map(|h| h.to_string())
                .chain(additional)
                .enumerate();
            &mut iter_binding_a
        }
        false => {
            iter_binding_b = csv_headers.iter().map(|h| h.to_string()).enumerate();
            &mut iter_binding_b
        }
    };
    // Deduplicate headers and build the Json sanitized headers list
    let headers = iter
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

    Ok((headers, records))
}

// JSON builder function, as JSON is a subset (mostly)
// of YAML this function also builds YAML representable data
pub fn compose(_opts: &ProgramArgs, data: (Vec<String>, Vec<Record>)) -> Vec<Map<String, JVal>> {
    let (header, record_list) = data;
    let hdr = header.iter().map(|s| &**s).collect::<Vec<&str>>();

    record_list
        .into_iter()
        .map(|record| {
            let mut headers = hdr.iter().take(record.field_count as usize);
            let mut records = record.data.iter();
            let mut output = Map::new();
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
                    output.insert(h_json.to_string(), JVal::String(r_json.to_string()));
                } else {
                    break;
                }
            }
            trace!("Map contents: {:?}", &output);

            output
        })
        .collect::<Vec<Map<String, JVal>>>()
}

// Serialization of the composed data occurs here
pub fn outwriter<W, S: ?Sized>(
    opts: &ProgramArgs,
    writer: W,
    output: &S,
) -> Result<(), Box<dyn Error>>
where
    W: ioWrite,
    S: Serialize,
{
    match opts.output_type() {
        OutputFormat::JsonPretty => match_with_log!(
            match serde_json::to_writer_pretty(writer, &output) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            },
            info!("Using pretty Json writer")
        ),
        OutputFormat::Json => match_with_log!(
            match serde_json::to_writer(writer, &output) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            },
            info!("Using Json writer")
        ),
        OutputFormat::Yaml => match_with_log!(
            match serde_yaml::to_writer(writer, &output) {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            },
            info!("Using Yaml writer")
        ),
    }
}
