use nom::{
  branch::alt,
  bytes::complete::{escaped, tag, take_while, take_while1, is_not},
  character::complete::{alphanumeric1 as alphanumeric, char, one_of, digit1, none_of},
  combinator::{map, opt, cut},
  error::{context},
  multi::{separated_list0, many0},
  number::complete::double,
  sequence::{delimited, preceded, pair, terminated},
  IResult,
};

// Only a few syntax forms, quote and quasiquote being the most unique,
// both of which map onto the regular list structure.
// Generous allotment of what's considered a symbol allows for the
// introduction of new syntax without having to actually parse for it,
// e.g. arrows, infix, function application, type annotations...

#[derive(Debug, PartialEq)]
pub enum Value {
  Symbol(String),
  Str(String),
  Boolean(bool),
  Int(i64),
  Frac(f64),
  List(Vec<Value>)
}

fn parse_str(i: &str) -> IResult<&str, &str> {
  escaped(alphanumeric, '\\', one_of("\"n\\"))(i)
}

fn space(i: &str) -> IResult<&str, &str> {
  let chars = " \t\r\n";
  take_while(move |c| chars.contains(c))(i)
}

fn parse_symbol (i: &str) -> IResult<&str, (char, Option<&str>)> {
    pair(
        none_of("\"'(), 0123456789"),
        opt(is_not("\"'(), "))
    )(i)
}

fn symbol (i: &str) -> IResult<&str, String> {
    map(parse_symbol, |(x, xs)| {
        let mut s = String::new();
        s.push(x);
        if xs.is_some() {
            s.push_str(xs.unwrap());
        }
        s
    })(i)
}

fn boolean(i: &str) -> IResult<&str, bool> {
  alt((
      map(tag("#false"), |_| false),
      map(tag("#true"), |_| true)
  ))(i)
}

fn string(i: &str) -> IResult<&str, &str> {
  context("string",
    preceded(
      char('\"'),
      cut(terminated(
          parse_str,
          char('\"')
  ))))(i)
}

fn integer(i: &str) -> IResult<&str, i64> {
    map(digit1, |s: &str| s.parse::<i64>().unwrap())(i)
}

fn list(i: &str) -> IResult<&str, Vec<Value>> {
    delimited(tag("("), many0(parse_value), tag(")"))(i)
}

pub fn parse_value(i: &str) -> IResult<&str, Value> {
  preceded(
    space,
    alt((
      map(string, |s| Value::Str(String::from(s))),
      map(integer, Value::Int),
      map(boolean, Value::Boolean),
      map(double, Value::Frac),
      map(symbol, Value::Symbol),
      map(list, Value::List),
    )),
  )(i)
}
