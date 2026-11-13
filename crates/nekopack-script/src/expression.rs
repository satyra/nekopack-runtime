//! Integer expressions over UTF-8 variable names and ASCII operators.

pub(crate) fn evaluate(
    input: &[u8],
    lookup: impl Fn(&[u8]) -> Result<i32, String>,
) -> Result<i32, String> {
    let mut parser = Parser {
        input,
        position: 0,
        lookup,
    };
    let value = parser.expression(1, true, 0)?;
    parser.whitespace();
    if parser.position != input.len() {
        return Err(format!(
            "unexpected expression input at byte {}",
            parser.position
        ));
    }
    Ok(value)
}

struct Parser<'a, F> {
    input: &'a [u8],
    position: usize,
    lookup: F,
}

#[derive(Clone, Copy)]
enum Operator {
    LogicalOr,
    LogicalAnd,
    Or,
    Xor,
    And,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

impl Operator {
    fn apply(self, left: i32, right: i32) -> Result<i32, String> {
        use Operator::*;
        let value = match self {
            LogicalOr => Some(i32::from(left != 0 || right != 0)),
            LogicalAnd => Some(i32::from(left != 0 && right != 0)),
            Or => Some(left | right),
            Xor => Some(left ^ right),
            And => Some(left & right),
            Equal => Some(i32::from(left == right)),
            NotEqual => Some(i32::from(left != right)),
            Less => Some(i32::from(left < right)),
            LessEqual => Some(i32::from(left <= right)),
            Greater => Some(i32::from(left > right)),
            GreaterEqual => Some(i32::from(left >= right)),
            Add => left.checked_add(right),
            Subtract => left.checked_sub(right),
            Multiply => left.checked_mul(right),
            Divide | Remainder if right == 0 => {
                return Err("integer expression division by zero".into())
            }
            Divide => left.checked_div(right),
            Remainder => left.checked_rem(right),
        };
        value.ok_or_else(|| "integer expression overflow".into())
    }
}

impl<F: Fn(&[u8]) -> Result<i32, String>> Parser<'_, F> {
    fn whitespace(&mut self) {
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn operator(&mut self) -> Option<(Operator, usize, u8)> {
        use Operator::*;
        self.whitespace();
        let tail = &self.input[self.position..];
        // Longest token first. Higher precedence binds more tightly.
        for (token, operator, precedence) in [
            (b"||".as_slice(), LogicalOr, 1),
            (b"&&", LogicalAnd, 2),
            (b"==", Equal, 6),
            (b"!=", NotEqual, 6),
            (b"<=", LessEqual, 7),
            (b">=", GreaterEqual, 7),
            (b"|", Or, 3),
            (b"^", Xor, 4),
            (b"&", And, 5),
            (b"<", Less, 7),
            (b">", Greater, 7),
            (b"+", Add, 8),
            (b"-", Subtract, 8),
            (b"*", Multiply, 9),
            (b"/", Divide, 9),
            (b"%", Remainder, 9),
        ] {
            if tail.starts_with(token) {
                return Some((operator, token.len(), precedence));
            }
        }
        None
    }

    fn expression(&mut self, minimum: u8, active: bool, depth: usize) -> Result<i32, String> {
        if depth >= 128 {
            return Err("integer expression nesting limit exceeded".into());
        }
        let mut left = self.unary(active, depth + 1)?;
        while let Some((operator, length, precedence)) = self.operator() {
            if precedence < minimum {
                break;
            }
            self.position += length;
            let right_active = active
                && match operator {
                    Operator::LogicalAnd => left != 0,
                    Operator::LogicalOr => left == 0,
                    _ => true,
                };
            let right = self.expression(precedence + 1, right_active, depth + 1)?;
            left = if active {
                operator.apply(left, right)?
            } else {
                0
            };
        }
        Ok(left)
    }

    fn unary(&mut self, active: bool, depth: usize) -> Result<i32, String> {
        if depth >= 128 {
            return Err("integer expression nesting limit exceeded".into());
        }
        self.whitespace();
        let byte = *self
            .input
            .get(self.position)
            .ok_or("integer expression operand missing")?;
        if matches!(byte, b'+' | b'-' | b'!' | b'~') {
            self.position += 1;
            self.whitespace();
            // The signed minimum is valid even though its positive magnitude
            // cannot be represented by i32.
            if byte == b'-'
                && self
                    .input
                    .get(self.position)
                    .is_some_and(u8::is_ascii_digit)
            {
                return self.decimal(true);
            }
            let value = self.unary(active, depth + 1)?;
            if !active {
                return Ok(0);
            }
            return match byte {
                b'+' => Ok(value),
                b'-' => value
                    .checked_neg()
                    .ok_or_else(|| "integer expression overflow".into()),
                b'!' => Ok(i32::from(value == 0)),
                b'~' => Ok(!value),
                _ => unreachable!(),
            };
        }
        if byte == b'(' {
            self.position += 1;
            let value = self.expression(1, active, depth + 1)?;
            self.whitespace();
            if self.input.get(self.position) != Some(&b')') {
                return Err("integer expression closing parenthesis missing".into());
            }
            self.position += 1;
            return Ok(value);
        }
        if byte.is_ascii_digit() {
            return self.decimal(false);
        }
        let start = self.position;
        while self
            .input
            .get(self.position)
            .is_some_and(|b| !b.is_ascii_whitespace() && !b"()+-*/%|&^=!<>~".contains(b))
        {
            self.position += 1;
        }
        if self.position == start {
            return Err(format!(
                "integer expression operand missing at byte {start}"
            ));
        }
        let name = &self.input[start..self.position];
        std::str::from_utf8(name).map_err(|_| "integer expression variable is not UTF-8")?;
        if active {
            (self.lookup)(name)
        } else {
            Ok(0)
        }
    }

    fn decimal(&mut self, negative: bool) -> Result<i32, String> {
        let start = self.position;
        while self
            .input
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        let digits = std::str::from_utf8(&self.input[start..self.position]).unwrap();
        let magnitude: i64 = digits
            .parse()
            .map_err(|_| "integer expression literal overflow")?;
        i32::try_from(if negative { -magnitude } else { magnitude })
            .map_err(|_| "integer expression literal overflow".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> Result<i32, String> {
        evaluate(input.as_bytes(), |name| match name {
            b"a" => Ok(8),
            b"b" => Ok(2),
            b"c" => Ok(2),
            _ if name == "ヒカリ".as_bytes() => Ok(5),
            _ if name == "计数".as_bytes() => Ok(11),
            _ => Err("unknown synthetic variable".into()),
        })
    }

    #[test]
    fn utf8_names_work_without_spaces() {
        assert_eq!(run("ヒカリ+1"), Ok(6));
        assert_eq!(run("计数-ヒカリ*2"), Ok(1));
        assert_eq!(run("\t计数 + (ヒカリ-1)\r\n"), Ok(15));
    }

    #[test]
    fn arithmetic_bitwise_and_comparison_precedence() {
        assert_eq!(run("2+3*4"), Ok(14));
        assert_eq!(run("20/3%4"), Ok(2));
        assert_eq!(run("10-3-2"), Ok(5));
        assert_eq!(run("1|2^3&6"), Ok(1));
        assert_eq!(run("1+2<4==1"), Ok(1));
        assert_eq!(run("3<=3 && 4>=3 && 3!=4 && 4>3"), Ok(1));
        assert_eq!(run("1|2==2"), Ok(1));
        assert_eq!(run("a | (b==c)"), Ok(9));
        assert_eq!(run("(a|b)==c"), Ok(0));
        assert_eq!(run("!-1 + ~0 + +2"), Ok(1));
    }

    #[test]
    fn logical_operators_short_circuit_but_validate_syntax() {
        assert_eq!(run("0&&missing"), Ok(0));
        assert_eq!(run("1||1/0"), Ok(1));
        assert_eq!(run("0||2&&3"), Ok(1));
        assert!(run("1||(2+").is_err());
    }

    #[test]
    fn integer_boundaries_and_invalid_inputs_are_errors() {
        assert_eq!(run("2147483647"), Ok(i32::MAX));
        assert_eq!(run("-2147483648"), Ok(i32::MIN));
        for input in [
            "",
            "()",
            "1+",
            "(1",
            "1)",
            "1 2",
            "2a",
            "1=2",
            "missing",
            "2147483648",
            "-2147483649",
            "2147483647+1",
            "-2147483648-1",
            "50000*50000",
            "-(-2147483648)",
            "-2147483648/-1",
            "1/0",
            "1%0",
            "999999999999999999999999999999999",
        ] {
            assert!(run(input).is_err(), "{input}");
        }
        assert!(evaluate(&[0xff], |_| Ok(1)).is_err());
        assert!(run(&format!("{}1{}", "(".repeat(128), ")".repeat(128))).is_err());
        assert!(run(&format!("{}1", "!".repeat(128))).is_err());
    }
}
