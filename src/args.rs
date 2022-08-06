use std::env;
use std::path::PathBuf;
use std::process::exit;
use std::str::FromStr;

use datetime::LocalDateTime;
use glob::glob;

use crate::chunk::ChunkInfo;
use crate::filter::FilterSet;
use crate::pretty::PrettyDescriptor;
use crate::translate::Translation;

const HELP: &str = r#"
saw SOURCE_FILES
  -h, --help [TOPIC]            Print help. If TOPIC is provided it will give more detail or list the topics
  -v, --version                 Prints the version of saw
  -p, --pretty [PATTERN]        Pretty print output as text instead of gzipped json PATTERN is optional and defines a pattern
  -f, --filter PATTERN          Filter based on contents, PATTERN defines how and what to match on
  -o, --output PATH             Instead of outputting to stdout, pipe results to a file directly
  -c, --chunked [SIZE]          Requires --output option. Chunks output into multiple files based on size or number of lines
  -r, --range MIN MAX           Filters logs to between the two given timestamps, (min is inclusive, max is exclusive)
  -t, --translate FIELD PATTERN Transform strings before printing them
  -z, --zip true|false          Gzip output. Defaults to true if output is provided and false otherwise
  -j, --json true|false         Output as JSON. Has defaults for all cases. Passing true while also providing pretty is illegal

help TOPIC values are:
  pretty    How pretty printing patterns work
  filter    How filtering patterns work
  range     How to use the range option
  translate How to use the translate feature (it's like sed for json)
  chunked   The syntax for chunked size limits
"#;

const PRETTY_TOPIC: &str = r#"
Usage:
  saw --pretty [PATTERN]

Pretty patterns are simply % prefixed JSON keys, constants and functions.

For example, the default pattern is "[%time] %message %prefix/\n/%stack\v/".

This prints the 'time' key surounded by square brackets, a space,
then the value of the message key, then the value of the stack key prefixed with a newline.
The \v seperates the %stack variable from the / because that's the syntax for a function.

If a field is missing, like stack, then an empty string will be used instead.

Excape characters are done with \.
Escapable charcters are:
t => tab
n => newline
r => carige return
s => ordinary space
v => nothing .. useful to terminate a pattern
/ => /
\ => \
% => %

Functions are used like:
%function/argument/another argument/

Function arguments might take strings or patterns.

Existing functions so far are:
%prefix/pattern to use as prefix/content pattern/
%replace/base pattern/regex/regex replacement/
%replaceAll/base pattern/regex/regex replacement/
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

Applying an empty filter works to confirm the field exists. For example: "%stack=" will print
all events that have a stack, regardless of what they contain.

A filter can be negated like this: "%message!=something". This will return all events where the message
does NOT contain the word "something".

To apply multiple filters, simply pass --filter more than once. These filters are always ANDed together.
There is currently no way to OR two filters. Multiple filters can touch the same or different fields.

For example: `saw -f Controller -f %stack=NullPointer` -f %level!=DEBUG will find all messages that contain the word
"Controller" and also have a stacktrace that contains the word "NullPointer" but who's level is NOT "DEBUG".
"#;

const RANGE_TOPIC: &str = r#"
Usage:
  saw --range MIN MAX

Range is used to select events from a specific time frame. The JSON field must be named "time".

The format must be ISO8601 local date time, which looks like this:
Example: "2020-03-01T12:00:00" which selects exactly noon on March 1st, 2020.

To break that down, it means: [year]-[month from 01-12]-[day from 01-31]T[hour from 01-24]:[minute from 00-59]:[second from 00-59]

Ranges must be exact, you can't leave off any part, not even the seconds at the end.
This is a likely area of improvement in the future.

You can however supply "*" as either the MIN or MAX to provide an open-ended time range.

Strictly speaking you can supply * for both MIN and MAX and this is equivalent to not providing a range at all.

MIN is inclusive, MAX is exclusive.
"#;

const TRANSLATE_TOPIC: &str = r#"
Usage:
  saw --translate TARGET_FIELD PATTERN

Used to transform values in the event before writing it.

The TARGET param is the key in the event that the result will be applied to.
Can be an existing or new field. If the PATTERN returns a blank string
then the field will be deleted. 'blank' is defined as a string contining only whitespace.

PATTERN is a pattern in exactly the same form as used in --pretty

Multiple translations can be applied by passing the argument more than once, and they will
be applied in order.
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
  line based one will once it has proccessed that many lines. Note that "lines" means lines of INPUT,
  or in other words JSON objects, not lines of OUTPUT in the case of using the pretty printer.

Examples:
  Rollover every 20 kilobytes: `saw --output ex --chunked 20kb`
  Rollover every 1000 lines: `saw --output ex --chunked 1000ln`
"#;

const DEFAULT_PRETTY: &str = "[%time] %message %prefix/\\n/%stack\\v/";

#[derive(Debug)]
pub struct Arguments {
  pub sources: Vec<PathBuf>,
  pub pretty: Option<PrettyDescriptor>,
  pub filter: Option<FilterSet>,
  pub output: Option<PathBuf>,
  pub chunked: Option<ChunkInfo>,
  pub translations: Vec<Translation>,
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
      translations: vec![],
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
              let message = match topic.as_ref() {
                "pretty"    => PRETTY_TOPIC,
                "filter"    => FILTER_TOPIC,
                "range"     => RANGE_TOPIC,
                "translate" => TRANSLATE_TOPIC,
                "chunked"   => CHUNKED_TOPIC,
                _           => HELP
              };

              println!("{}", message);
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
                init.pretty = Some(Arguments::load_default_pattern());
              } else {
                init.pretty = Some(PrettyDescriptor::parse(&src.next().unwrap()));
              }
            } else {
              init.pretty = Some(Arguments::load_default_pattern());
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
          "-t" | "--translate" => {
            let output = src.next().expect("Argument --translate must be followed by a TARGET_FIELD and then a PATTERN argument");
            let pattern = src.next().expect("Argument --translate TARGET_FIELD must be followed by a PATTERN argument");

            let translation = Translation::parse(output, &pattern);

            init.translations.push(translation);
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
          init.pretty = Some(Arguments::load_default_pattern());
        }
      }
    } else {
      // if you did not specify json

      if init.output.is_none() {
        // if you did not provide output

        // if you did not specify pretty, default it on
        if init.pretty.is_none() {
          init.pretty = Some(Arguments::load_default_pattern());
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

  /**
   * Either load up the default from an environment variable or take the default provided
   */
  fn load_default_pattern() -> PrettyDescriptor {
    return env::var("SAW_PATTERN")
      .map(| it | PrettyDescriptor::parse(&it))
      .unwrap_or(PrettyDescriptor::parse(DEFAULT_PRETTY));
  }
}
