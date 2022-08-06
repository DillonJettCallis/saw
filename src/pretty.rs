use std::collections::HashMap;
use std::io::Write;
use std::iter::Peekable;
use std::str::Chars;
use std::vec::IntoIter;
use regex::Regex;

use serde_json::{Map, Value};

use crate::utils::ExtraIter;

#[derive(Debug, Clone)]
pub struct PrettyDescriptor {
  fragments: Vec<PrettyFragment>,
}

#[derive(Debug, Clone)]
enum PrettyFragment {
  Literal(String),
  Variable(String),
  Prefix {
    prefix: PrettyDescriptor,
    base: PrettyDescriptor
  },
  Replace {
    base: PrettyDescriptor,
    regex: Regex,
    replacement: String,
    global: bool,
  }
}

#[derive(Debug, Clone)]
enum PrettyToken {
  Literal(String),
  Variable(String),
  Slash,
}

/*

Example:
%message %prefix/a thing/%stack/

Given:
{"message": "plain", "stack": "stacktrace here"}

Prints:
plain a thing stacktrace here

Given:
{"message": "plain"}

Prints:
plain

*/

lazy_static! {
  static ref ESCAPE_MAP: HashMap<char, char> = HashMap::from([
    ('t', '\t'),
    ('s', ' '),
    ('n', '\n'),
    ('r', '\r'),
    ('%', '%'),
    ('\\', '\\'),
    ('/', '/'),
  ]);
}

/**

pattern looks like this:

%message [%thread] - %stack

% followed by letters is a variable, everything else is a literal

if the variable is missing, an empty string will be used
 */

impl PrettyDescriptor {
  pub fn parse(pattern: &str) -> PrettyDescriptor {
    let tokens = PrettyDescriptor::lex(pattern);
    let mut src = tokens.into_iter().peekable();

    let mut fragments = Vec::new();

    while let Some(frag) = PrettyDescriptor::parse_expression(&mut src) {
      fragments.push(frag);
    }

    PrettyDescriptor { fragments }
  }

  fn parse_expression(src: &mut Peekable<IntoIter<PrettyToken>>) -> Option<PrettyFragment> {
    if let Some(next) = src.next() {
      let ans = match next {
        PrettyToken::Literal(lit) => PrettyFragment::Literal(lit),
        PrettyToken::Variable(name) => {
          if let Some(PrettyToken::Slash) = src.peek() {
            src.next();
            PrettyDescriptor::parse_function(src, &name)
          } else {
            PrettyFragment::Variable(name)
          }
        }
        PrettyToken::Slash => panic!("Unexpected '/' found in pattern! Did you mean to escape it?"),
      };

      Some(ans)
    } else {
      None
    }
  }

  fn parse_function(src: &mut Peekable<IntoIter<PrettyToken>>, name: &str) -> PrettyFragment {
    match name {
      "prefix" => {
        let prefix = PrettyDescriptor::parse_pattern_argument(src);
        let base = PrettyDescriptor::parse_pattern_argument(src);

        PrettyFragment::Prefix { prefix, base }
      }
      "replace" | "replaceAll" => {
        let base = PrettyDescriptor::parse_pattern_argument(src);
        let regex_pattern = PrettyDescriptor::parse_literal_argument(src);
        let replacement = PrettyDescriptor::parse_literal_argument(src);

        let regex = Regex::new(&regex_pattern).expect("%regex pattern is invalid!");

        PrettyFragment::Replace {
          base,
          regex,
          replacement,
          global: name == "replaceAll"
        }
      }
      _ => panic!("Unknown function call in pattern! '{name}' is not a known function, see `saw --help pretty` for list of functions")
    }
  }

  fn parse_pattern_argument(src: &mut Peekable<IntoIter<PrettyToken>>) -> PrettyDescriptor {
    let mut fragments = Vec::<PrettyFragment>::new();

    loop {
      if let PrettyToken::Slash = src.peek().expect("Pattern contains unterminated function call") {
        src.next();
        return PrettyDescriptor{fragments};
      } else {
        if let Some(frag) = PrettyDescriptor::parse_expression(src) {
          fragments.push(frag)
        } else {
          panic!("Pattern contains unterminated function call")
        }
      }
    }
  }

  fn parse_literal_argument(src: &mut Peekable<IntoIter<PrettyToken>>) -> String {
    let regex_pattern = if let Some(PrettyToken::Literal(lit)) = src.next() {
      lit
    } else {
      panic!("Expected string argument to function")
    };

    if let Some(PrettyToken::Slash) = src.next() {
    } else {
      panic!("Function ended unexpectedly");
    }

    return regex_pattern;
  }

  fn lex(pattern: &str) -> Vec<PrettyToken> {
    let mut tokens: Vec<PrettyToken> = vec![];

    let mut src = pattern.chars().peekable();

    while let Some(next) = src.peek() {
      match next {
        '%' => {
          src.next();
          let mut name = String::new();
          PrettyDescriptor::lex_identifier(&mut src, &mut name);
          tokens.push(PrettyToken::Variable(name));
        }
        '/' => {
          src.next();
          tokens.push(PrettyToken::Slash)
        },
        _ => {
          let mut literal = String::new();
          PrettyDescriptor::lex_literal(&mut src, &mut literal);
          tokens.push(PrettyToken::Literal(literal));
        }
      }
    }

    return tokens;
  }

  fn lex_identifier(src: &mut Peekable<Chars>, name: &mut String) {
    while let Some(next @ ('a'..='z' | 'A'..='Z')) = src.peek() {
      name.push(next.clone());
      src.next();
    }
  }

  fn lex_literal(src: &mut Peekable<Chars>, name: &mut String) {
    while let Some(next) = src.peek() {
      match next {
        '\\' => {
          src.next(); // discard the slash
          let follow = src.next().expect("Pattern cannot end with an unmatched '\\' character.");

          if follow == 'v' {
            return;
          }

          let found = ESCAPE_MAP.get(&follow).expect(&format!("Pattern contained unknown and invalid escape sequence '{follow}'"));
          name.push(found.clone());
        }
        '%' | '/' => {
          return;
        }
        _ => {
          name.push(next.clone());
          src.next();
        }
      }
    }
  }

  pub fn print<Writer: Write>(&self, values: &Map<String, Value>, target: &mut Writer) -> () {
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
        PrettyFragment::Prefix { prefix, base } => {
          let result = base.print_to_string(values);
          let trimmed = result.trim();

          if !trimmed.is_empty() {
            prefix.print(values, target);
            target.write_all(trimmed.as_bytes()).expect("Failed to write");
          }
        }
        PrettyFragment::Replace { base, regex, replacement, global } => {
          let content = base.print_to_string(values);

          let replaced = if *global {
            regex.replace_all(&content, replacement)
          } else {
            regex.replace(&content, replacement)
          };

          target.write_all(replaced.as_bytes()).expect("Failed to write")
        }
      };
    }
  }

  pub fn print_to_string(&self, values: &Map<String, Value>) -> String {
    let mut out = Vec::new();

    self.print(values, &mut out);

    // should be safe to unwrap because we wrote these bytes, there shouldn't be controversy about if they're valid or not
    String::from_utf8(out).unwrap()
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
