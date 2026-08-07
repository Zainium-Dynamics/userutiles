//! user expr — evaluate expressions (integer + string core).

use usercore::Ui;

/// Entry point for the `expr` utility. Parses `std::env::args()` as a
/// single EXPRESSION (tokens are whitespace-separated shell arguments, not
/// re-split), evaluates it, and prints the result.
///
/// Returns 0 if the result is neither empty nor `0`, 1 if it is, or 2 on a
/// syntax/evaluation error (matching GNU `expr`'s exit-code convention).
pub fn run() -> i32 {
    let ui = Ui::new("expr");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        if args.is_empty() {
            ui.err("missing operand");
            return 2;
        }
        print!(
            "Usage: expr EXPRESSION\n\
 Evaluate EXPRESSION and print result.\n\
 Support: + - * / % ( ) comparisons length index substr\n"
        );
        return 0;
    }
    if args[0] == "--version" {
        println!("expr (user_utils) 0.1.0");
        return 0;
    }

    match eval_tokens(&args) {
        Ok(Val::Int(n)) => {
            println!("{n}");
            if n == 0 {
                1
            } else {
                0
            }
        }
        Ok(Val::Str(s)) => {
            println!("{s}");
            if s.is_empty() || s == "0" {
                1
            } else {
                0
            }
        }
        Err(e) => {
            ui.err(&e);
            2
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Int(i64),
    Str(String),
}

impl Val {
    fn as_int(&self) -> Result<i64, String> {
        match self {
            Val::Int(n) => Ok(*n),
            Val::Str(s) => s.parse().map_err(|_| "non-integer argument".to_string()),
        }
    }
    fn as_str(&self) -> String {
        match self {
            Val::Int(n) => n.to_string(),
            Val::Str(s) => s.clone(),
        }
    }
    fn is_null(&self) -> bool {
        match self {
            Val::Int(0) => true,
            Val::Str(s) if s.is_empty() || s == "0" => true,
            _ => false,
        }
    }
}

struct Parser {
    toks: Vec<String>,
    i: usize,
}

/// Parse and evaluate a full expression from `toks`. Errors if the tokens
/// don't form a complete expression (trailing garbage) or contain a
/// syntax/arithmetic error (division by zero, overflow, etc.).
fn eval_tokens(toks: &[String]) -> Result<Val, String> {
    let mut p = Parser {
        toks: toks.to_vec(),
        i: 0,
    };
    let v = p.parse_or()?;
    if p.i < p.toks.len() {
        return Err("syntax error".into());
    }
    Ok(v)
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.i).map(|s| s.as_str())
    }

    fn bump(&mut self) -> Option<String> {
        if self.i < self.toks.len() {
            let t = self.toks[self.i].clone();
            self.i += 1;
            Some(t)
        } else {
            None
        }
    }

    fn parse_or(&mut self) -> Result<Val, String> {
        let mut left = self.parse_and()?;
        while self.peek() == Some("|") {
            self.bump();
            let right = self.parse_and()?;
            left = if left.is_null() { right } else { left };
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Val, String> {
        let mut left = self.parse_cmp()?;
        while self.peek() == Some("&") {
            self.bump();
            let right = self.parse_cmp()?;
            left = if left.is_null() || right.is_null() {
                Val::Int(0)
            } else {
                left
            };
        }
        Ok(left)
    }

    fn parse_cmp(&mut self) -> Result<Val, String> {
        let left = self.parse_add()?;
        let op = self.peek().unwrap_or("").to_string();
        if matches!(op.as_str(), "=" | "!=" | "<" | "<=" | ">" | ">=") {
            self.bump();
            let right = self.parse_add()?;
            let res = match (left.as_int(), right.as_int()) {
                (Ok(a), Ok(b)) => match op.as_str() {
                    "=" => a == b,
                    "!=" => a != b,
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    ">=" => a >= b,
                    _ => false,
                },
                _ => {
                    let a = left.as_str();
                    let b = right.as_str();
                    match op.as_str() {
                        "=" => a == b,
                        "!=" => a != b,
                        "<" => a < b,
                        "<=" => a <= b,
                        ">" => a > b,
                        ">=" => a >= b,
                        _ => false,
                    }
                }
            };
            return Ok(Val::Int(if res { 1 } else { 0 }));
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Val, String> {
        let mut left = self.parse_mul()?;
        loop {
            match self.peek() {
                Some("+") => {
                    self.bump();
                    let r = self.parse_mul()?.as_int()?;
                    left = Val::Int(left.as_int()?.checked_add(r).ok_or("integer overflow")?);
                }
                Some("-") => {
                    self.bump();
                    let r = self.parse_mul()?.as_int()?;
                    left = Val::Int(left.as_int()?.checked_sub(r).ok_or("integer overflow")?);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Val, String> {
        let mut left = self.parse_primary()?;
        loop {
            match self.peek() {
                Some("*") | Some("\\*") => {
                    self.bump();
                    let r = self.parse_primary()?.as_int()?;
                    left = Val::Int(left.as_int()?.checked_mul(r).ok_or("integer overflow")?);
                }
                Some("/") => {
                    self.bump();
                    let r = self.parse_primary()?.as_int()?;
                    if r == 0 {
                        return Err("division by zero".into());
                    }
                    left = Val::Int(left.as_int()?.checked_div(r).ok_or("integer overflow")?);
                }
                Some("%") => {
                    self.bump();
                    let r = self.parse_primary()?.as_int()?;
                    if r == 0 {
                        return Err("division by zero".into());
                    }
                    left = Val::Int(left.as_int()?.checked_rem(r).ok_or("integer overflow")?);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Val, String> {
        let tok = match self.bump() {
            Some(t) => t,
            None => return Err("syntax error".into()),
        };
        match tok.as_str() {
            "(" => {
                let v = self.parse_or()?;
                if self.bump().as_deref() != Some(")") {
                    return Err("syntax error: expected ')'".into());
                }
                Ok(v)
            }
            "length" => {
                let s = self.parse_primary()?.as_str();
                Ok(Val::Int(s.chars().count() as i64))
            }
            "index" => {
                let s = self.parse_primary()?.as_str();
                let chars = self.parse_primary()?.as_str();
                let pos = s
                    .chars()
                    .position(|c| chars.contains(c))
                    .map(|i| (i + 1) as i64)
                    .unwrap_or(0);
                Ok(Val::Int(pos))
            }
            "substr" => {
                let s = self.parse_primary()?.as_str();
                let pos = self.parse_primary()?.as_int()?;
                let len = self.parse_primary()?.as_int()?;
                if pos <= 0 || len <= 0 {
                    return Ok(Val::Str(String::new()));
                }
                let sub: String = s
                    .chars()
                    .skip((pos as usize).saturating_sub(1))
                    .take(len as usize)
                    .collect();
                Ok(Val::Str(sub))
            }
            "+" => {
                let s = self.parse_primary()?.as_str();
                Ok(Val::Str(s))
            }
            t => {
                if let Ok(n) = t.parse::<i64>() {
                    Ok(Val::Int(n))
                } else {
                    Ok(Val::Str(t.to_string()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(args: &[&str]) -> Result<Val, String> {
        let toks: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        eval_tokens(&toks)
    }

    #[test]
    fn eval_simple_addition() {
        assert_eq!(eval(&["1", "+", "2"]).unwrap(), Val::Int(3));
    }

    #[test]
    fn eval_precedence_mul_before_add() {
        assert_eq!(eval(&["2", "+", "3", "*", "4"]).unwrap(), Val::Int(14));
    }

    #[test]
    fn eval_parens_override_precedence() {
        assert_eq!(
            eval(&["(", "2", "+", "3", ")", "*", "4"]).unwrap(),
            Val::Int(20)
        );
    }

    #[test]
    fn eval_division_by_zero_errors() {
        assert_eq!(eval(&["1", "/", "0"]).unwrap_err(), "division by zero");
    }

    #[test]
    fn eval_modulo_by_zero_errors() {
        assert_eq!(eval(&["1", "%", "0"]).unwrap_err(), "division by zero");
    }

    #[test]
    fn eval_addition_overflow_errors() {
        assert!(eval(&[&i64::MAX.to_string(), "+", "1"]).is_err());
    }

    #[test]
    fn eval_division_overflow_does_not_panic() {
        // i64::MIN / -1 overflows a signed division; must be a caught
        // error, not a panic.
        assert!(eval(&[&i64::MIN.to_string(), "/", "-1"]).is_err());
    }

    #[test]
    fn eval_length_counts_chars() {
        assert_eq!(eval(&["length", "hello"]).unwrap(), Val::Int(5));
    }

    #[test]
    fn eval_substr_extracts_range() {
        assert_eq!(
            eval(&["substr", "hello", "2", "3"]).unwrap(),
            Val::Str("ell".to_string())
        );
    }

    #[test]
    fn eval_substr_negative_len_is_empty() {
        assert_eq!(
            eval(&["substr", "hello", "2", "-1"]).unwrap(),
            Val::Str(String::new())
        );
    }

    #[test]
    fn eval_string_equality() {
        assert_eq!(eval(&["foo", "=", "foo"]).unwrap(), Val::Int(1));
        assert_eq!(eval(&["foo", "=", "bar"]).unwrap(), Val::Int(0));
    }

    #[test]
    fn eval_numeric_comparison() {
        assert_eq!(eval(&["10", ">", "9"]).unwrap(), Val::Int(1));
    }

    #[test]
    fn eval_trailing_garbage_is_syntax_error() {
        assert!(eval(&["1", "+", "1", "2"]).is_err());
    }

    #[test]
    fn eval_empty_is_syntax_error() {
        assert!(eval(&[]).is_err());
    }

    #[test]
    fn is_null_treats_zero_and_empty_as_null() {
        assert!(Val::Int(0).is_null());
        assert!(Val::Str("0".to_string()).is_null());
        assert!(Val::Str(String::new()).is_null());
        assert!(!Val::Int(1).is_null());
        assert!(!Val::Str("x".to_string()).is_null());
    }
}
