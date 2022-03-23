use std::env;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

use datetime::LocalDateTime;
use glob::{glob, Pattern};

use crate::chunk::ChunkInfo;
use crate::filter::FilterSet;
use crate::pretty::PrettyDescriptor;

const HELP: &str = r#"
saw SOURCE_FILES
   -h, --help [TOPIC]     Print help. If TOPIC is provided it will give more detail or list the topics
   -v, --version          Prints the version of saw
   -p, --pretty [PATTERN] Pretty print output as text instead of gzipped json PATTERN is optional and defines a pattern
   -f, --filter PATTERN   Filter based on contents, PATTERN defines how and what to match on
   -o, --output PATH      Instead of outputting to stdout, pipe results to a file directly
   -c, --chunked [SIZE]   Requires --output option. Chunks output into multiple files based on size or number of lines
   -r, --range MIN MAX    Filters logs to between the two given timestamps, (min is inclusive, max is exclusive)
   -z, --zip true|false   Gzip output. Defaults to true if output is provided and false otherwise
   -j, --json true|false  Output as JSON. Has defaults for all cases. Passing true while also providing pretty is illegal

help TOPIC values are:
  pretty    How pretty printing patterns work
  filter    How filtering patterns work
  chunked   THe syntax for chunked size limits
"#;

const PRETTY_TOPIC: &str = r#"
Usage:
  saw --pretty [PATTERN]

Pretty patterns are simply % prefixed JSON keys and constants.

For example, the default pattern is "[%time] %message %stack".

This prints the 'time' key surounded by square brackets, a space,
then the value of the message key, then the value of the stack key.

If a field is missing, like stack, then an empty string will be used instead.
"#;

const FILTER_TOPIC: &str = r#"
Usage:
  saw --filter PATTERN

Filter patterns are based on the Rust Regex crate: https://docs.rs/regex/latest/regex/
This crate's Regexes follow in a standard Perl-inspired syntax and support most features
of any other regex engine.

You can supply any valid regex and by default the "message" value will be compared to see
if it _contains_ the given regex. If you want to specify that the regex must match completely,
simply use the regex start '^' and end '$' tokens, as specified in that documentation.

For example, the message: "This is a log message" will match the pattern "log"
but will not match the pattern "log$" because that one requires that "log"
is the final word in the message.

To test a field other than 'message', simply append a '%' prefixed key and an '=', then follow with the pattern.

For example: "%stack=NullPointer" will match any stack field that contains the word "NullPointer"

If a log does not contain a 'stack' field, it is automatically excluded. There is no way to
apply a filter conditionally.

To apply multiple filters, simply pass --filter more than once. These filters are always ANDed together.
There is currently no way to OR two filters. Multiple filters can touch the same or different fields.

For example: `saw -f Controller -f %stack=NullPointer` will find all messages that contain the word
"Controler" and also have a stacktrace that contains the word "NullPointer".
"#;

const CHUNKED_TOPIC: &str = r#"
Usage:
  saw --output file --chunked SIZE

Because saw can read in multiple files and merge them all together, the output might
be larger than you would want to store in a single output file. The --chunked option
solves this by automatically splitting the output based on file size.

The SIZE param is made of a number followed without space by a letter code specifying
what kind of value was passed.

Options for this code are
  b: Bytes
  kb: Kilobytes
  mb: Megabytes
  gb: Gigabytes
  ln: Lines

  The byte bases ones will create a new file once the old file exceded the given limit, and the
  line based one will once it has proccessed that many lines. Not that "lines" means lines of INPUT,
  or in other words JSON objects, not lines of OUTPUT in the case of using the pretty printer.

Examples:
  Rollover every 20 kilobytes: `saw --output ex --chunked 20kb`
  Rollover every 1000 lines: `saw --output ex --chunked 1000ln`
"#;

const DEFAULT_PRETTY: &str = "[%time] %message %stack";

#[derive(Debug)]
pub struct Arguments {
  pub sources: Vec<PathBuf>,
  pub pretty: Option<PrettyDescriptor>,
  pub filter: Option<FilterSet>,
  pub output: Option<PathBuf>,
  pub chunked: Option<ChunkInfo>,
  pub range: (Option<LocalDateTime>, Option<LocalDateTime>),
  pub zip: bool,
}

impl Arguments {
  pub fn parse() -> Arguments {
    let mut init = Arguments {
      sources: vec![],
      pretty: None,
      filter: None,
      output: None,
      chunked: None,
      range: (None, None),
      zip: false,
    };

    // have these flags been passed?
    let mut has_zip = false;
    let mut has_json = false;

    // json is not on Arguments because the outer code can assume Pretty OR JSON
    let mut json = false;

    let mut src = env::args().peekable();

    // the first argument is the program, always ignore that.
    src.next();

    while let Some(next) = src.next() {
      if next.starts_with("-") {
        match next.as_ref() {
          "-h" | "--help" => {
            if let Some(topic) = src.next() {
              match topic.as_ref() {
                "pretty" => eprintln!("{}", PRETTY_TOPIC),
                "filter" => eprintln!("{}", FILTER_TOPIC),
                "chunked" => eprintln!("{}", CHUNKED_TOPIC),
                _ => eprintln!("{}", HELP)
              }
              exit(0)
            }

            eprintln!("{}", HELP);
            exit(0);
          }
          "-v" | "--version" => {
            eprintln!("0.1.0");
            exit(0);
          }
          "-p" | "--pretty" => {
            if init.pretty.is_some() {
              panic!("Cannot pass argument --pretty twice!")
            }

            if let Some(pattern) = src.peek() {
              if pattern.starts_with('-') {
                init.pretty = Some(PrettyDescriptor::parse(DEFAULT_PRETTY));
              } else {
                init.pretty = Some(PrettyDescriptor::parse(&src.next().unwrap()));
              }
            } else {
              init.pretty = Some(PrettyDescriptor::parse(DEFAULT_PRETTY));
            }
          }
          "-f" | "--filter" => {
            let raw = src
              .next()
              .expect("Argument --filter must be followed by a pattern");

            let filter = FilterSet::parse(&raw);

            if let Some(set) = &mut init.filter {
              set.sets.push(filter);
            } else {
              init.filter = Some(FilterSet{ sets: vec![filter] });
            }
          }
          "-o" | "--output" => {
            if init.output.is_some() {
              panic!("Cannot pass argument --filter twice!")
            }

            init.output = Some(
              src.next()
                .expect("Argument --output must be followed by a file path")
                .into(),
            )
          }
          "-c" | "--chunked" => {
            if init.chunked.is_some() {
              panic!("Cannot pass argument --filter twice!")
            }

            let raw = src
              .next()
              .expect("Argument --chunked must be followed by a size descriptor");

            init.chunked = Some(ChunkInfo::parse(&raw))
          }
          "-z" | "--zip" => {
            if has_zip {
              panic!("Cannot pass argument --zip twice!")
            }

            has_zip = true;

            let raw = src.next().expect("Argument --zip must be followed by 'true' or 'false'");

            let value = match raw.to_lowercase().as_str() {
              "true" => true,
              "false" => false,
              _ => panic!("Argument --zip must be followed by 'true' or 'false'")
            };

            init.zip = value;
          }
          "-j" | "--json" => {
            if has_json {
              panic!("Cannot pass argument --json twice!")
            }

            has_json = true;

            let raw = src.next().expect("Argument --json must be followed by 'true' or 'false'");

            json = match raw.to_lowercase().as_str() {
              "true" => true,
              "false" => false,
              _ => panic!("Argument --json must be followed by 'true' or 'false'")
            };
          }
          "-r" | "--range" => {
            if let (None, None) = init.range {} else {
              panic!("Cannot pass argument --range twice!")
            }

            let raw_min = src.next().expect(
              "Argument --range must be followed by a MIN and then MAX value",
            );
            let raw_max = src
              .next()
              .expect("Argument --range MIN must be followed by a MAX value");

            let range = match (raw_min.as_ref(), raw_max.as_ref()) {
              ("*", "*") => (None, None),
              ("*", raw_max) => {
                let max = LocalDateTime::from_str(raw_max).expect(
                  "Argument --range MAX must be a valid ISO8601 local date time",
                );

                (None, Some(max))
              }
              (raw_min, "*") => {
                let min = LocalDateTime::from_str(raw_min).expect(
                  "Argument --range MIN must be a valid ISO8601 local date time",
                );

                (Some(min), None)
              }
              (raw_min, raw_max) => {
                let min = LocalDateTime::from_str(raw_min).expect(
                  "Argument --range MIN must be a valid ISO8601 local date time",
                );
                let max = LocalDateTime::from_str(raw_max).expect(
                  "Argument --range MAX must be a valid ISO8601 local date time",
                );

                (Some(min), Some(max))
              }
            };

            init.range = range
          }
          _ => {
            panic!("Unknown property '{next}'. Run saw with --help to see all known properties");
          }
        }
      }

      // must be a source
      init.sources.append(&mut Arguments::read_path(&next));
    }

    // a few remaining defaults and sanity checks

    // chunked requires output
    if init.chunked.is_some() && init.output.is_none() {
      panic!("Option --chunked is only valid when option --output is specified!");
    }

    if has_json {
      // if you passed the json flag

      if json {
        // if you passed true

        if init.pretty.is_some() {
          // and pretty is on
          panic!("Cannot pass both --pretty and --json true at the same time as these options conflict")
        }
      } else {
        // if you specified json false, we need to default pretty if you did not
        if init.pretty.is_none() {
          init.pretty = Some(PrettyDescriptor::parse(DEFAULT_PRETTY));
        }
      }
    } else {
      // if you did not specify json

      if init.output.is_none() {
        // if you did not provide output

        // if you did not specify pretty, default it on
        if init.pretty.is_none() {
          init.pretty = Some(PrettyDescriptor::parse(DEFAULT_PRETTY));
        }
      }
    }

    // if you did not specify zip
    if !has_zip {
      // set zip on if pretty it off
      init.zip = init.pretty.is_none()
    }

    return init;
  }

  fn read_path(raw: &str) -> Vec<PathBuf> {
    glob(raw)
      .expect(&format!(
        "Source '{raw}' is not valid or directory could not be read"
      ))
      .map(|p| p.expect(&format!("Source '{raw}' is not valid or could not be read")))
      .collect()
  }
}
