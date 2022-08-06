use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, stdin};
use std::path::PathBuf;
use std::str::FromStr;

use datetime::LocalDateTime;
use flate2::read::GzDecoder;
use serde_json::{Map, Value};

pub struct FileSource {
  pub file: String,
  pub line: u64,
}

pub struct Line {
  pub value: Map<String, Value>,
  pub time: LocalDateTime,
  pub src: FileSource,
}

pub struct LogFile {
  src: Box<dyn BufRead>,
  name: String,
  line: u64,

  is_completed: bool,
  pub next: Option<Line>,
}

const GZIP_MAGIC: [u8; 2] = [31u8, 139u8];

impl LogFile {

  pub fn from_file(path: &PathBuf) -> LogFile {
    let name = path.to_str().unwrap_or("<invalid path>").to_string();
    let mut file = File::open(path).expect(&format!("Failed to open file {name}"));
    let mut gzip_check = [0u8; 2];
    let read = file
      .read(&mut gzip_check)
      .expect(&format!("Failed to open file {name}"));
    file.rewind().expect("Failed to rewind file!");

    let src: Box<dyn BufRead> = if read != 2 || GZIP_MAGIC != gzip_check {
      // not gzip
      Box::new(BufReader::new(file))
    } else {
      Box::new(BufReader::new(GzDecoder::new(file)))
    };

    LogFile {
      src,
      name,
      line: 0,
      is_completed: false,
      next: None,
    }
  }

  pub fn from_stdin() -> LogFile {
    let src = Box::new(BufReader::new(stdin()));

    LogFile {
      src,
      name: "<stdin>".to_string(),
      line: 0,
      is_completed: false,
      next: None,
    }
  }

  pub fn time(&self) -> LocalDateTime {
    if self.is_completed {
      panic!("Attempt to peek at a completed LogFile!")
    }

    self.next.as_ref().unwrap().time
  }

  /**
   * Take the next line. Only call this after a call to advance returns true.
   * Calling this without calling advance will panic. Calling this twice in a row will panic.
   */
  pub fn take(&mut self) -> Line {
    if self.is_completed {
      panic!("Attempt to take at a completed LogFile!")
    }

    self.next.take().unwrap()
  }

  /**
   * Read in the next value. Returns true if a value was read, false if the EOF was reached.
   */
  pub fn advance(&mut self) -> bool {
    if self.is_completed {
      return false;
    }

    // do this until do_advance returns true
    while !self.do_advance() {}

    // do_advance will set this flag
    !self.is_completed
  }

  // returns true if a value was successfully read, false if something went wrong with the line.
  fn do_advance(&mut self) -> bool {
    let mut raw = String::new();
    let read = self.src
      .read_line(&mut raw)
      .expect(&format!("Failed to read line from file {}", self.name));
    let file = self.name.clone();
    let line = self.line;
    self.line += 1;

    if read == 0 {
      // EOF
      self.is_completed = true;
      return true;
    }

    let body = match serde_json::from_str(&raw) {
      Ok(Value::Object(map)) => map,
      _ => {
        eprintln!("Invalid JSON in file '{file}' at line {line}");
        return false;
      }
    };

    let time = match &body.get("time") // pluck time out
      .and_then(|time| time.as_str()) // convert it to a string
      .and_then(|time| LocalDateTime::from_str(time).ok()) // convert to type
    {
      Some(time) => time.clone(),
      None => {
        eprintln!("Invalid or missing 'time' field in JSON from file '{file}' at line {line}");
        return false;
      }
    };

    let src = FileSource { file, line };

    self.next = Some(Line {
      value: body,
      time,
      src,
    });

    // successfully read a value
    return true;
  }
}

pub struct Aggregator {
  logs: Vec<LogFile>,
}

impl Aggregator {
  pub fn new(mut logs: Vec<LogFile>) -> Aggregator {
    // load up initial values and remove any that are empty
    logs.iter_mut().for_each(|log| {
      log.advance();
      ()
    });

    // keep only those that are not completed
    logs.retain(|log| !log.is_completed);

    // sort them most oldest first
    logs.sort_unstable_by(|left, right| right.time().cmp(&left.time()));

    Aggregator { logs }
  }
}

impl Iterator for Aggregator {
  type Item = Line;

  fn next(&mut self) -> Option<Self::Item> {
    if self.logs.is_empty() {
      return None;
    }

    let (min_index, min) = self
      .logs
      .iter_mut()
      .enumerate()
      .min_by(|(_, l), (_, r)| l.time().cmp(&r.time()))
      .unwrap();

    let result = min.take();

    // if advance returns null it means that this file is empty
    if !min.advance() {
      self.logs.remove(min_index);
    }

    Some(result)
  }
}

