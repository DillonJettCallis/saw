use std::env;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

use datetime::LocalDateTime;
use glob::{glob, Pattern};

use crate::chunk::ChunkInfo;
use crate::pretty::PrettyDescriptor;

const HELP: &str = r#"
saw SOURCE_FILES
   -h, --help             Prints this help dialog
   -v, --version          Prints the version of saw
   -p, --pretty [PATTERN] Pretty print output as text instead of gzipped json PATTERN is optional and defines a pattern.
   -f, --filter PATTERN   Filter based on contents, PATTERN defines how and what to match on
   -o, --output PATH      Instead of outputting to stdout, pipe results to a file directly
   -c, --chunked [SIZE]   Requires --output option. Chunks output into multiple files based on size or number of lines.
   -r, --range MIN MAX    Filters logs to between the two given timestamps, (min is inclusive, max is exclusive)
"#;

const DEFAULT_PRETTY: &str = "[%time] %message %stack";

#[derive(Debug)]
pub struct Arguments {
  pub sources: Vec<PathBuf>,
  pub pretty: Option<PrettyDescriptor>,
  pub filter: Option<Pattern>,
  pub output: Option<PathBuf>,
  pub chunked: Option<ChunkInfo>,
  pub range: (Option<LocalDateTime>, Option<LocalDateTime>),
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
    };

    let mut src = env::args().peekable();

    // the first argument is the program, always ignore that.
    src.next();

    while let Some(next) = src.next() {
      if next.starts_with("-") {
        match next.as_ref() {
          "-h" | "--help" => {
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
            if init.filter.is_some() {
              panic!("Cannot pass argument --filter twice!")
            }
            let raw = src
              .next()
              .expect("Argument --filter must be followed by a pattern");
            let pattern = Pattern::new(&raw)
              .expect(&format!("Filter argument '{raw}' is not a valid pattern"));

            init.filter = Some(pattern)
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

    // a few remaining sanity checks
    if init.chunked.is_some() && init.output.is_none() {
      panic!("Option --chunked is only valid when option --output is specified!");
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
