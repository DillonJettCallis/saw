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
    let mut src = raw.chars();
    let mut number = String::new();

    while let Some(next @ '0'..='9') = src.next() {
      number.push(next);
    }

    let mut suffix = String::new();

    while let Some(next @ 'a'..='z') = src.next() {
      suffix.push(next);
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
