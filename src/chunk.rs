use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use flate2::Compression;
use flate2::write::GzEncoder;

#[derive(Debug)]
pub struct ChunkInfo {
  pub value: usize,
  pub unit: ChunkUnit,
}

#[derive(Debug)]
pub enum ChunkUnit {
  Bytes,
  Lines,
}

const BYTE_SUFFIXES: [(&str, usize); 4] = [
  ("b", 1),
  ("kb", 1024),
  ("mb", 1024 * 1024),
  ("gb", 1024 * 1024 * 1024),
];

const LINE_SUFFIX: &str = "ln";

impl ChunkInfo {
  pub fn parse(raw: &str) -> ChunkInfo {
    let mut src = raw.chars().peekable();
    let mut number = String::new();

    while let Some('0'..='9') = src.peek() {
      number.push(src.next().unwrap());
    }

    let mut suffix = String::new();

    while let Some('a'..='z') = src.peek() {
      suffix.push(src.next().unwrap());
    }

    if let Some(_) = src.next() {
      panic!("Invalid chunk pattern {raw}");
    }

    let raw_value: usize = number
      .parse()
      .expect(&format!("Chunk number {number} is not a valid number"));

    if suffix == LINE_SUFFIX {
      return ChunkInfo {
        value: raw_value,
        unit: ChunkUnit::Lines,
      };
    }

    for (key, multiplier) in BYTE_SUFFIXES {
      if suffix == key {
        let value = raw_value.checked_mul(multiplier)
          .expect(&format!("Chunk value {raw} is too large! Try trimming the value down to something more reasonable (the max unsigned value your arch can represent)"));

        return ChunkInfo {
          value,
          unit: ChunkUnit::Bytes,
        };
      }
    }

    let all_suffixes: Vec<String> = BYTE_SUFFIXES.iter().map(|(s, _)| s.to_string()).collect();

    panic!(
      "Chunk suffix {suffix} is not recognized. Valid options are {}, {}",
      LINE_SUFFIX,
      all_suffixes.join(", ")
    )
  }
}

struct NoOpWriter {}

impl Write for NoOpWriter {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    Ok(buf.len())
  }

  fn flush(&mut self) -> std::io::Result<()> {
    Ok(())
  }
}

pub struct ChunkedWriter {
  base_path: PathBuf,
  chunk_info: ChunkInfo,
  zipped: bool,
  chunk_index: usize,
  written: usize,
  inner: Box<dyn Write>,
}

impl ChunkedWriter {

  pub fn new(base_path: PathBuf, chunk_info: ChunkInfo, zipped: bool) -> ChunkedWriter {
    let mut res = ChunkedWriter {
      base_path,
      chunk_info,
      zipped,
      chunk_index: 0,
      written: 0,
      inner: Box::new(NoOpWriter{}), // just a placeholder, we update it instantly
    };

    // this fills inner with an actual valid value
    res.next_chunk();

    res
  }

  fn next_chunk(&mut self) {
    let index = self.chunk_index;
    let ext = if self.zipped { ".log.gz" } else { ".log" };

    let base_file_name = self.base_path.file_name().unwrap().to_str().unwrap();
    let file_name = base_file_name.to_owned() + "." + &index.to_string() + ext;

    let file_path = self.base_path.with_file_name(file_name);

    self.chunk_index += 1;
    let file = BufWriter::new(File::create(file_path).expect(&format!("Failed to create file '{}.{}'", self.base_path.to_str().unwrap_or("<invalid>"), self.chunk_index)));

    if self.zipped {
      self.inner = Box::new(GzEncoder::new(file, Compression::best()))
    } else {
      self.inner = Box::new(file)
    }
  }
}

impl Write for ChunkedWriter {

  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    let written = self.inner.write(buf);

    // if we're counting bytes then all the whole size, else we're counting lines
    if let ChunkUnit::Bytes = self.chunk_info.unit {
      self.written += written.as_ref().expect("Failed to write to file");
    }

    written
  }

  fn flush(&mut self) -> std::io::Result<()> {
    self.inner.flush()
  }
}

pub trait LogWriter: Write {
  fn end_line(&mut self) {
    self.write(b"\n").expect("Failed to write to file");
  }
}

impl LogWriter for ChunkedWriter {

  fn end_line(&mut self) {
    self.write(b"\n").expect("Failed to write to file");

    if let ChunkUnit::Lines = self.chunk_info.unit {
      self.written += 1;
    }

    if self.written >= self.chunk_info.value {
      self.next_chunk();
      self.written = 0;
    }
  }
}

impl <Inner: Write> LogWriter for GzEncoder<Inner> {}
impl <Inner: Write> LogWriter for BufWriter<Inner> {}

