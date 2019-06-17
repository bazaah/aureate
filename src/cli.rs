use {
    crate::models::{
        assets::{OutputFormat, ReadFrom},
        get_reader,
    },
    clap::{crate_authors, crate_version, App, Arg, ArgMatches as Matches, SubCommand},
    csv::Trim,
    simplelog::LevelFilter,
    std::boxed::Box,
};

pub fn generate_cli<'a>() -> Matches<'a> {
    let matches = App::new("aureate")
        .about("Utility for converting CSV to JSON/YAML")
        .author(crate_authors!("\n"))
        .version(crate_version!())
        .arg(
            Arg::with_name("verbosity")
                .short("v")
                .multiple(true)
                .max_values(4)
                .takes_value(false)
                .help("Sets level of debug output"),
        )
        .arg(Arg::with_name("quiet")
                .short("q")
                .long("quiet")
                .takes_value(false)
                .help("Silences error messages")
        )
        .arg(Arg::with_name("append")
                .short("a")
                .long("append")
                .takes_value(false)
                .help("Append to output file, instead of overwriting")
                .long_help("Append to output file, instead of overwriting... has no effect if writing to stdout")
        )
        .arg(
            Arg::with_name("format")
                .short("f")
                .long("format")
                .takes_value(true)
                .possible_values(&["prettyj", "json", "yaml"])
                .default_value("prettyj")
                .help("Set output data format"),
        )
        .arg(
            Arg::with_name("input")
                .short("i")
                .long("input")
                .value_name("FILE")
                .takes_value(true)
                .multiple(true)
                .require_delimiter(true)
                .help("Input file path(s) separated by commas, with a '-' representing stdin"),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILE")
                .takes_value(true)
                .help("Specify an output file path, defaults to stdout"),
        )
        .subcommand(
            SubCommand::with_name("csv")
                .about("Settings related to fine-tuning the CSV reader")
                .alias(" ")
                .after_help("NOTE: options which take <CHAR> accept one and only one character... everything else will be dropped!")
                .arg(
                    Arg::with_name("delimiter_csv")
                        .short("s")
                        .long("delimiter")
                        .takes_value(true)
                        .default_value(",")
                        .value_name("CHAR")
                        .help("Specify your CSV delimiter"),
                )
                .arg(
                    Arg::with_name("flexible_csv")
                        .long("flexible")
                        .takes_value(false)
                        .help("Prevents program from erroring on non-uniform row fields")
                        .long_help("Normally CSV is considered malformed if each record does not have the same number of fields. This setting allows for parsing of such data sets")
                )
                .arg(
                    Arg::with_name("trim_settings_csv")
                        .short("t")
                        .long("trim")
                        .default_value("0")
                        .value_name("SETTING")
                        .validator(|s: String| {
                            match s.as_str() {
                                "0" | "1" | "2" | "3" | "none" | "headers" | "fields" | "all" => Ok(()),
                                _ => Err(format!("Invalid setting:\n['0' | 'none', '1' | 'headers', '2' | 'fields', '3' | 'all']"))
                            }
                        })
                        .help("Set CSV trimming")
                        .long_help("Possible values: ['0' | 'none', '1' | 'headers', '2' | 'fields', '3' | 'all']")
                )
                .arg(
                    Arg::with_name("comment_csv")
                        .short("c")
                        .long("comment")
                        .takes_value(true)
                        .value_name("CHAR")
                        .help("Specify your CSV comment character")
                )
                .arg(
                    Arg::with_name("quote_settings_csv")
                        .long("disable-quotes")
                        .takes_value(true)
                        .possible_values(&["double", "all"])
                        .value_name("SETTING")
                        .help("Disables quote handling")
                        .long_help("Disables quote handling, either for double quotes only, or for all quotes")
                )
                .arg(
                    Arg::with_name("quote_csv")
                        .short("q")
                        .long("quote")
                        .default_value("\"")
                        .value_name("CHAR")
                        .help("Specify your CSV quote character")
                )
                .arg(
                    Arg::with_name("escape_csv")
                        .short("e")
                        .long("escape")
                        .takes_value(true)
                        .value_name("CHAR")
                        .help("Specify your CSV escape character")
                )
        )
        .get_matches();

    matches
}

pub struct ProgramArgs<'a> {
    // Program
    _store: Matches<'a>,
    debug_level: LevelFilter,
    output_type: OutputFormat,
    reader: Vec<Option<ReadFrom>>,
    writer: (Option<String>, bool),
    // CSV
    flexible_csv: CSVOption,
    delimiter_csv: CSVOption,
    escape_csv: CSVOption,
    comment_csv: CSVOption,
    quote_csv: CSVOption,
    trim_settings_csv: CSVOption,
    quote_settings_csv: CSVOption,
}

impl<'a> ProgramArgs<'a> {
    pub fn init(store: Matches<'a>) -> Self {
        let debug_level = match (store.occurrences_of("verbosity"), store.is_present("quiet")) {
            (_, true) => LevelFilter::Off,
            (0, false) => LevelFilter::Info,
            (1, false) => LevelFilter::Debug,
            (_, false) => LevelFilter::Trace,
        };

        let output_type = match store.value_of("format") {
            Some("prettyj") => OutputFormat::JsonPretty,
            Some("json") => OutputFormat::Json,
            Some("yaml") => OutputFormat::Yaml,
            _ => unreachable!(),
        };

        let reader = match store.values_of("input") {
            Some(inputs) => {
                let mut list: Vec<_> = inputs.collect();
                list.dedup_by_key(|f| *f == "-");
                list.iter()
                    .map(|s| get_reader(Some(s)))
                    .collect::<Vec<Option<ReadFrom>>>()
            }
            None => {
                let mut vec: Vec<Option<ReadFrom>> = Vec::new();
                let i = get_reader(None);
                vec.push(i);
                vec
            }
        };
        let writer = match (store.value_of("output"), store.is_present("append")) {
            (Some(s), false) => (Some(s.to_string()), false),
            (Some(s), true) => (Some(s.to_string()), true),
            (None, _) => (None, false),
        };

        // CSV reader options
        /* ---------------------------------------- */

        let flexible_csv: CSVOption;
        let delimiter_csv: CSVOption;
        let escape_csv: CSVOption;
        let comment_csv: CSVOption;
        let quote_csv: CSVOption;
        let trim_settings_csv: CSVOption;
        let quote_settings_csv: CSVOption;

        match store.subcommand_matches("csv") {
            Some(csv) => {
                flexible_csv = CSVOption::Flexible(csv.is_present("flexible_csv"));

                delimiter_csv = CSVOption::DelimiterChar(match csv.value_of("delimiter_csv") {
                    Some("\\t") => "\t".bytes().nth(0).unwrap(),
                    Some(s) => s.bytes().nth(0).unwrap(),
                    _ => unreachable!(),
                });

                escape_csv = CSVOption::EscapeChar(match csv.value_of("escape_csv") {
                    Some("\\t") => Some("\t".bytes().nth(0).unwrap()),
                    Some(s) => Some(s.bytes().nth(0).unwrap()),
                    None => None,
                });

                comment_csv = CSVOption::CommentChar(match csv.value_of("comment_csv") {
                    Some("\\t") => Some("\t".bytes().nth(0).unwrap()),
                    Some(s) => Some(s.bytes().nth(0).unwrap()),
                    None => None,
                });

                quote_csv = CSVOption::QuoteChar(match csv.value_of("quote_csv") {
                    Some("\\t") => "\t".bytes().nth(0).unwrap(),
                    Some(s) => s.bytes().nth(0).unwrap(),
                    _ => unreachable!(),
                });

                trim_settings_csv =
                    CSVOption::TrimSettings(match csv.value_of("trim_settings_csv") {
                        Some(level) => match level {
                            "0" | "none" => Trim::None,
                            "1" | "headers" => Trim::Headers,
                            "2" | "fields" => Trim::Fields,
                            "3" | "all" => Trim::All,
                            _ => unreachable!(),
                        },
                        _ => unreachable!(),
                    });

                quote_settings_csv =
                    CSVOption::QuoteSettings(match csv.value_of("quote_settings_csv") {
                        Some(level) => match level {
                            "double" => (false, true),
                            "all" => (false, false),
                            _ => unreachable!(),
                        },
                        None => (true, true),
                    });
            }
            None => {
                flexible_csv = CSVOption::Flexible(false);
                delimiter_csv = CSVOption::DelimiterChar(b',');
                escape_csv = CSVOption::EscapeChar(None);
                comment_csv = CSVOption::CommentChar(None);
                quote_csv = CSVOption::QuoteChar(b'"');
                trim_settings_csv = CSVOption::TrimSettings(Trim::None);
                quote_settings_csv = CSVOption::QuoteSettings((true, true));
            }
        }
        /* ---------------------------------------- */

        Self {
            //Program Options
            _store: store,
            debug_level,
            output_type,
            reader,
            writer,

            //CSV Options
            flexible_csv,
            delimiter_csv,
            escape_csv,
            comment_csv,
            quote_csv,
            trim_settings_csv,
            quote_settings_csv,
        }
    }

    pub fn debug_level(&self) -> LevelFilter {
        self.debug_level
    }

    pub fn output_type(&self) -> OutputFormat {
        self.output_type
    }

    pub fn reader_list(&self) -> &Vec<Option<ReadFrom>> {
        &self.reader
    }

    pub fn writer(&self) -> &(Option<String>, bool) {
        &self.writer
    }

    // CSV.ReaderBuilder related methods

    pub fn delimiter(&self) -> u8 {
        self.delimiter_csv.into()
    }

    pub fn flexible(&self) -> bool {
        self.flexible_csv.into()
    }

    pub fn escape(&self) -> Option<u8> {
        self.escape_csv.into()
    }

    pub fn comment(&self) -> Option<u8> {
        self.comment_csv.into()
    }

    pub fn quote(&self) -> u8 {
        self.quote_csv.into()
    }

    pub fn trim_settings(&self) -> Trim {
        self.trim_settings_csv.into()
    }

    pub fn quote_settings(&self) -> (bool, bool) {
        self.quote_settings_csv.into()
    }
}

#[derive(Debug, Clone, Copy)]
enum CSVOption {
    Flexible(bool),
    DelimiterChar(u8),
    EscapeChar(Option<u8>),
    CommentChar(Option<u8>),
    QuoteChar(u8),
    TrimSettings(Trim),
    QuoteSettings((bool, bool)),
}

impl From<CSVOption> for u8 {
    fn from(opt: CSVOption) -> Self {
        match opt {
            CSVOption::DelimiterChar(bytes) => bytes,
            CSVOption::QuoteChar(bytes) => bytes,
            _ => unreachable!(),
        }
    }
}

impl From<CSVOption> for Option<u8> {
    fn from(opt: CSVOption) -> Self {
        match opt {
            CSVOption::CommentChar(o) => o,
            CSVOption::EscapeChar(o) => o,
            _ => unreachable!(),
        }
    }
}

impl From<CSVOption> for bool {
    fn from(opt: CSVOption) -> Self {
        match opt {
            CSVOption::Flexible(bl) => bl,
            _ => unreachable!(),
        }
    }
}

impl From<CSVOption> for (bool, bool) {
    fn from(opt: CSVOption) -> Self {
        match opt {
            CSVOption::QuoteSettings(tuple) => tuple,
            _ => unreachable!(),
        }
    }
}

impl From<CSVOption> for Trim {
    fn from(opt: CSVOption) -> Self {
        match opt {
            CSVOption::TrimSettings(trim) => trim,
            _ => unreachable!(),
        }
    }
}
