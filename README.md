# aureate

Aureate is an data enrichment tool for converting CSV into JSON and YAML

It is written is pure rust and makes use of the [serde](https://serde.rs/) library family for de/serialization, and [simplelog](https://github.com/Drakulix/simplelog.rs) for logging

### USAGE

    aureate [FLAGS] [OPTIONS] [SUBCOMMAND]

#### FLAGS

* `-h, --help`       Prints help information
* `-V, --version`    Prints version information
* `-v ...`           Sets level of debug output 

#### OPTIONS

* `-f, --format <format>`    Set output data format [default: prettyj]  [possible values: prettyj, json, yaml]
* `-i, --input <FILE>...`    Input file path(s) separated by commas, with a '-' representing stdin
* `-o, --output <FILE>`      Specify an output file path, defaults to stdout

#### SUBCOMMANDS

1. ### csv

    Settings related to fine-tuning the CSV reader

    * #### USAGE:

            aureate csv [FLAGS] [OPTIONS]

    * #### FLAGS:
        * `--flexible`    Prevents program from erroring on non-uniform row fields

    * #### OPTIONS:

        * `-c, --comment <CHAR>`              Specify your CSV comment character
        * `-s, --delimiter <CHAR>`            Specify your CSV delimiter [default: ,]
        * `-e, --escape <CHAR>`               Specify your CSV escape character
        * `-q, --quote <CHAR>`                Specify your CSV quote character [default: "]
        * `--disable-quotes <SETTING>`        Disables quote handling [possible values: double, all]
        * `-t, --trim <SETTING>`              Set CSV trimming [default: 0]

2. ### help

    Prints help of the given subcommand

    * #### USAGE:

            aureate help [SUBCOMMAND]