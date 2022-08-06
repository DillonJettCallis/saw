use serde_json::{Map, Value};
use crate::PrettyDescriptor;

#[derive(Debug)]
pub struct Translation {
  output: String,
  pattern: PrettyDescriptor,
}

impl Translation {

  pub fn parse(output: String, raw: &str) -> Translation {
    Translation {
      output,
      pattern: PrettyDescriptor::parse(raw),
    }
  }

  pub fn translate(&self, values: &mut Map<String, Value>) {
    let result = self.pattern.print_to_string(values);

    if result.trim().is_empty() {
      values.remove(&self.output);
    } else {
      values.insert(self.output.clone(), Value::String(result));
    }
  }
}
