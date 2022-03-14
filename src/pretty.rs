use serde_json::{Map, Value};
use std::io::Write;
use std::ops::Add;
use crate::utils::StringIter;
use crate::utils::ExtraIter;

#[derive(Debug)]
pub struct PrettyDescriptor {
  fragments: Vec<PrettyFragment>,
}

#[derive(Debug, Clone)]
enum PrettyFragment {
  Literal(String),
  Variable(String),
}

/**

pattern looks like this:

%message [%thread] - %stack

% followed by letters is a variable, everything else is a literal

if the variable is missing, an empty string will be used
 */

impl PrettyDescriptor {
  pub fn parse(pattern: &str) -> PrettyDescriptor {
    let mut fragments: Vec<PrettyFragment> = vec![];

    let mut src = pattern.chars().peekable();

    let mut state = PrettyFragment::Literal(String::new());

    while let Some(next) = src.next() {
      match (&mut state, next) {
        (frag, '%') => {
          fragments.push(frag.clone());
          state = PrettyFragment::Variable(String::new());
        }
        (PrettyFragment::Literal(lit), next) => {
          lit.push(next);
        }
        (PrettyFragment::Variable(name), next @ 'a'..='z' | next @ 'A'..='Z') => {
          name.push(next);
        }
        (var @ PrettyFragment::Variable(_), next) => {
          fragments.push(var.clone());
          state = PrettyFragment::Literal(next.to_string());
        }
      }
    }

    fragments.push(state);

    PrettyDescriptor { fragments }
  }

  pub fn print<Writer: Write>(&self, values: Map<String, Value>, target: &mut Writer) -> () {
    for frag in &self.fragments {
      match &frag {
        PrettyFragment::Literal(lit) => {
          target.write_all(lit.as_bytes()).expect("Failed to write")
        }
        PrettyFragment::Variable(name) => {
          if let Some(value) = values.get(name).map(PrettyDescriptor::pretty_value) {
            target.write_all(value.as_bytes()).expect("Failed to write")
          }
        }
      };
    }

    target.write(b"\n").expect("Failed to write");
  }

  fn pretty_value(value: &Value) -> String {
    match value {
      Value::String(str) => str.to_string(),
      Value::Number(num) => num.to_string(),
      Value::Null => "".to_string(),
      Value::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
      Value::Array(arr) => {
        arr.iter().join(", ", |v| PrettyDescriptor::pretty_value(v))
      }
      Value::Object(obj) => {
        obj.iter().join(", ", | (k, v) | {
          format!("{k}: {}", PrettyDescriptor::pretty_value(v))
        })
      }
    }
  }


}
