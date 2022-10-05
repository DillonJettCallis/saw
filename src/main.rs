extern crate core;
#[macro_use]
extern crate lazy_static;

use std::fs::File;
use std::io::{BufWriter, stdout, Write};
use std::path::PathBuf;

use datetime::LocalDateTime;
use flate2::Compression;
use flate2::write::GzEncoder;

use args::Arguments;

use crate::chunk::{ChunkedWriter, ChunkInfo, LogWriter};
use crate::filter::FilterSet;
use crate::log::{Aggregator, Line, LogFile};
use crate::pretty::PrettyDescriptor;
use crate::translate::Translation;

mod args;
mod chunk;
mod filter;
mod log;
mod pretty;
mod translate;
mod utils;

fn main() {
  let args = Arguments::parse();

  let mut agg = Aggregator::new(args.sources);

  if args.daily {
    agg.filter_daily(args.range);
  }

  let ranged = do_range(agg, args.range);
  let filtered = do_filter(ranged, args.filter);
  let translated = do_translate(filtered, args.translations);
  let writer = handle_output(args.output, args.chunked, args.zip);
  do_pretty(translated, args.pretty, writer);
}

fn do_filter<Iter: 'static + Iterator<Item=Line>>(
  src: Iter,
  maybe_pattern: Option<FilterSet>,
) -> Box<dyn Iterator<Item=Line>> {
  if let Some(filter) = maybe_pattern {
    Box::new(src.filter(move |row| {
      filter.matches(&row.value)
    }))
  } else {
    Box::new(src)
  }
}

fn do_range<Iter: 'static + Iterator<Item=Line>>(
  src: Iter,
  maybe_range: (Option<LocalDateTime>, Option<LocalDateTime>),
) -> Box<dyn Iterator<Item=Line>> {
  match maybe_range {
    (None, None) => Box::new(src),
    (Some(min), None) => {
      let range = min..;

      Box::new(src.filter(move |line| range.contains(&line.time)))
    }
    (None, Some(max)) => {
      let range = ..max;

      Box::new(src.filter(move |line| range.contains(&line.time)))
    }
    (Some(min), Some(max)) => {
      let range = min..max;

      Box::new(src.filter(move |line| range.contains(&line.time)))
    }
  }
}

fn do_translate<Iter: 'static + Iterator<Item=Line>>(
  src: Iter,
  translations: Vec<Translation>,
) -> Box<dyn Iterator<Item=Line>> {
  if translations.is_empty() {
    return Box::new(src);
  }

  return Box::new(src.map(move |mut line| {
    for trans in &translations {
      trans.translate(&mut line.value);
    }

    line
  }));
}

fn handle_output(maybe_output: Option<PathBuf>, chunked: Option<ChunkInfo>, zipped: bool) -> Box<dyn LogWriter> {
  if let Some(output) = maybe_output {
    if let Some(chunk_info) = chunked {
      Box::new(ChunkedWriter::new(output, chunk_info, zipped))
    } else {
      let target = File::create(output).expect("Could not create output file");

      handle_zip(BufWriter::new(target), zipped)
    }
  } else {
    handle_zip(BufWriter::new(stdout()), zipped)
  }
}

fn handle_zip<Writer: 'static + Write + LogWriter>(src: Writer, zip: bool) -> Box<dyn LogWriter> {
  if zip {
    Box::new(GzEncoder::new(src, Compression::best()))
  } else {
    Box::new(src)
  }
}

fn do_pretty<Iter: 'static + Iterator<Item=Line>>(
  src: Iter,
  maybe_pretty: Option<PrettyDescriptor>,
  mut target: Box<dyn LogWriter>,
) {
  if let Some(pretty) = maybe_pretty {
    src.for_each(move |line| {
      pretty.print(&line.value, &mut target);
      target.end_line();
    })
  } else {
    src.for_each(move |line| {
      serde_json::to_writer(&mut target, &line.value).expect("Failed to write line");
      target.end_line();
    })
  }
}
