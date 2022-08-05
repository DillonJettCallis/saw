use regex::Regex;
use serde_json::{Map, Value};

#[derive(Debug)]
pub struct FilterSet {
  pub sets: Vec<Filter>,
}

#[derive(Debug)]
pub struct Filter {
  key: String,
  inverse: bool,
  pattern: Regex,
}

lazy_static! {
  static ref PATTERN: Regex = Regex::new(r"^(%(\w+)(!)?=)?(.*)$").unwrap();
}

impl FilterSet {

  pub fn matches(&self, line: &Map<String, Value>) -> bool {
    self.sets.iter().fold(true, | sum, next | {
      if sum {
        if let Some(value) = line.get(&next.key) {
          if let Some(base) = value.as_str() {
            next.pattern.is_match(base) ^ next.inverse
          } else {
            false
          }
        } else {
          false
        }
      } else {
        false
      }
    })
  }

  pub fn parse(base: &str) -> Filter {
    let captures = PATTERN.captures(base).expect(&format!("Filter input {base} does not match valid pattern. Run saw --help filter for more information"));

    let key = captures.get(2).map_or("message", |m| m.as_str()).to_owned();
    let inverse = captures.get(3).is_some();
    let body = captures.get(4).expect(&format!("Filter input {base} does not match valid pattern. Run saw --help filter for more information"))
      .as_str();

    let pattern = Regex::new(body).expect(&format!("Filter is not a valid regex according to https://github.com/rust-lang/regex"));


    Filter{key, inverse, pattern}
  }
}
