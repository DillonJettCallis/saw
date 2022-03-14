use std::ops::Add;

pub trait StringIter {
  fn join(self, deliminator: &str) -> String;
}

impl <'a, Iter> StringIter for Iter where Iter: 'a + Iterator<Item=&'a str> {

  fn join(mut self, deliminator: &str) -> String {
    let mut result = String::new();

    if let Some(first) = self.next() {
      result.push_str(first);
    } else {
      return result;
    }

    for next in self.next() {
      result.push_str(deliminator);
      result.push_str(next);
    }

    return result;
  }

}

pub trait ExtraIter<Item> {
  fn join<Mapper: Fn(Item) -> String>(self, deliminator: &str, mapper: Mapper) -> String;
}

impl <Item, Iter> ExtraIter<Item> for Iter where Iter: Iterator<Item=Item> {
  fn join<Mapper: Fn(Item) -> String>(mut self, deliminator: &str, mapper: Mapper) -> String {
    let mut result = String::new();

    if let Some(first) = self.next() {
      result.push_str(&mapper(first));
    } else {
      return result;
    }

    for next in self.next() {
      result.push_str(deliminator);
      result.push_str(&mapper(next));
    }

    return result;
  }
}
