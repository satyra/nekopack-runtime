//! Byte-preserving bootstrap parser, cooperative scenario state, and system bridge.
//! Parsing performs no CP932/CP936 conversion; unsupported syntax stays explicit.
use std::{fmt, ops::Range};
pub mod checkpoint;
mod expression;
pub mod runtime;
pub mod system;

#[derive(Debug, PartialEq, Eq)]
pub enum Statement<'a> {
    Assign {
        name: &'a [u8],
        value: i32,
    },
    Define {
        name: &'a [u8],
        value: i32,
    },
    Variables {
        bank: u8,
        first: u32,
        names: Vec<&'a [u8]>,
    },
    Call(&'a [u8]),
    Load(&'a [u8]),
    System(&'a [u8]),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Node<'a> {
    /// One-based physical source line, before skipping comments or blanks.
    pub line: usize,
    /// Byte span of the trimmed statement, excluding newline and indentation.
    pub span: Range<usize>,
    /// Original tokens, including the dot-prefixed command.
    pub tokens: Vec<&'a [u8]>,
    pub statement: Statement<'a>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub offset: usize,
    pub reason: &'static str,
    pub token: Vec<u8>,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "script line {} byte 0x{:X}: {}",
            self.line, self.offset, self.reason
        )?;
        write!(f, " (token hex ")?;
        for b in &self.token {
            write!(f, "{b:02X}")?;
        }
        write!(f, ")")
    }
}
impl std::error::Error for Error {}

fn space(b: u8) -> bool {
    b == b' ' || b == b'\t'
}
fn decimal(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
}

/// Native banks each contain 1000 slots. F/f retain distinct GF/LF identities.
pub(crate) fn parse_slot_address(address: &[u8]) -> Result<(u8, u32), &'static str> {
    let (bank, digits) = if let Some(n) = address.strip_prefix(b"GF") {
        (b'F', n)
    } else if let Some(n) = address.strip_prefix(b"LF") {
        (b'f', n)
    } else if let Some(n) = address.strip_prefix(b"G") {
        (b'G', n)
    } else if let Some(n) = address.strip_prefix(b"L") {
        (b'L', n)
    } else {
        return Err("variable address requires G/GF/L/LF bank");
    };
    if !decimal(digits) {
        return Err("variable address requires decimal index");
    }
    let index = std::str::from_utf8(digits).unwrap().parse::<u32>()
        .map_err(|_| "slot address overflow")?;
    if index >= 1000 {
        return Err("variable index outside 0..999");
    }
    Ok((bank, index))
}

/// Parse .def, .var, .call, .load, .system and `? name = integer` bootstrap syntax.
/// Accept LF/CRLF, ASCII space/tab, and whole-line semicolon comments.
/// Unsupported syntax is an error, never silently skipped or treated as dialogue.
pub fn parse_bootstrap(data: &[u8], max_statements: usize) -> Result<Vec<Node<'_>>, Error> {
    let mut result = Vec::new();
    let mut offset = 0;
    for (line_index, physical) in data.split_inclusive(|b| *b == b'\n').enumerate() {
        let line = line_index + 1;
        let body = physical.strip_suffix(b"\n").unwrap_or(physical);
        let body = body.strip_suffix(b"\r").unwrap_or(body);
        let start = body.iter().position(|b| !space(*b)).unwrap_or(body.len());
        let end = body
            .iter()
            .rposition(|b| !space(*b))
            .map_or(start, |i| i + 1);
        let raw = &body[start..end];
        let fail = |reason| Error {
            line,
            offset: offset + start,
            reason,
            token: raw.split(|b| space(*b)).next().unwrap_or_default().to_vec(),
        };
        if raw.is_empty() || raw[0] == b';' {
            offset += physical.len();
            continue;
        }
        if result.len() == max_statements {
            return Err(fail("statement budget exceeded"));
        }
        if raw
            .iter()
            .any(|b| matches!(b, 0..=8 | 10..=31 | 127 | b'"' | b'\'' | b';'))
        {
            return Err(fail("unsupported control, quote, or inline comment syntax"));
        }
        let mut tokens: Vec<&[u8]> = raw.split(|b| space(*b)).filter(|t| !t.is_empty()).collect();
        if tokens[0].starts_with(b"?") && tokens[0].len() > 1 {
            let first = tokens[0];
            tokens[0] = &first[..1];
            tokens.insert(1, &first[1..]);
        }
        let args = &tokens[1..];
        let statement = match tokens[0] {
            b"?" => {
                if args.len() != 3 || args[1] != b"=" {
                    return Err(fail("assignment subset requires ? name = decimal"));
                }
                let digits = args[2].strip_prefix(b"-").unwrap_or(args[2]);
                if !decimal(digits) {
                    return Err(fail("assignment value is not a decimal integer"));
                }
                let value = std::str::from_utf8(args[2])
                    .unwrap()
                    .parse()
                    .map_err(|_| fail("assignment value outside i32"))?;
                Statement::Assign {
                    name: args[0],
                    value,
                }
            }
            b".def" => {
                if args.len() != 2 {
                    return Err(fail(".def needs name and decimal value"));
                }
                let digits = args[1].strip_prefix(b"-").unwrap_or(args[1]);
                if !decimal(digits) {
                    return Err(fail(".def value is not a decimal integer"));
                }
                let value = std::str::from_utf8(args[1])
                    .unwrap()
                    .parse()
                    .map_err(|_| fail(".def value outside i32"))?;
                Statement::Define {
                    name: args[0],
                    value,
                }
            }
            b".var" => {
                if args.len() < 2 {
                    return Err(fail(".var needs address and at least one name"));
                }
                let (bank, first) = parse_slot_address(args[0]).map_err(fail)?;
                if args.len() - 1 > (1000 - first) as usize {
                    return Err(fail("variable declaration crosses bank boundary"));
                }
                Statement::Variables {
                    bank,
                    first,
                    names: args[1..].to_vec(),
                }
            }
            b".call" | b".load" | b".system" => {
                if args.len() != 1 {
                    return Err(fail("resource command needs exactly one target"));
                }
                match tokens[0] {
                    b".call" => Statement::Call(args[0]),
                    b".load" => Statement::Load(args[0]),
                    _ => Statement::System(args[0]),
                }
            }
            _ => return Err(fail("unsupported bootstrap statement")),
        };
        result.push(Node {
            line,
            span: offset + start..offset + end,
            tokens,
            statement,
        });
        offset += physical.len();
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patch_assignment_is_not_a_general_expression_parser() {
        assert_eq!(
            parse_bootstrap(b"?x = 3", 1).unwrap()[0].statement,
            Statement::Assign {
                name: b"x",
                value: 3
            }
        );
        let nodes = parse_bootstrap(b"? \x90\x85 = 0", 1).unwrap();
        assert_eq!(
            nodes[0].statement,
            Statement::Assign {
                name: b"\x90\x85",
                value: 0
            }
        );
        for bad in ["? x == 0", "? x = y", "? x = 1+2", "? x = 2147483648"] {
            assert!(parse_bootstrap(bad.as_bytes(), 1).is_err());
        }
        let e = parse_bootstrap(b".future x", 1).unwrap_err();
        assert_eq!(e.token, b".future");
        assert!(e.to_string().contains("2E667574757265"));
    }
    #[test]
    fn borrowed_bytes_spans_and_physical_lines() {
        let data = b";comment\r\n\r\n\t.def \x95\x5c -2\r\n.var G10 \x82\xa0 second\n.system gui.nut\n.call patch\n.load title";
        let nodes = parse_bootstrap(data, 5).unwrap();
        assert_eq!(nodes.len(), 5);
        assert_eq!(nodes[0].line, 3);
        assert_eq!(&data[nodes[0].span.clone()], b".def \x95\x5c -2");
        assert_eq!(
            nodes[0].statement,
            Statement::Define {
                name: b"\x95\x5c",
                value: -2
            }
        );
        assert_eq!(
            nodes[1].statement,
            Statement::Variables {
                bank: b'G',
                first: 10,
                names: vec![b"\x82\xa0", b"second"]
            }
        );
        assert_eq!(nodes[4].statement, Statement::Load(b"title"));
        assert!(parse_bootstrap(data, 4).is_err());
    }
    #[test]
    fn unknowns_and_invalid_forms_fail_without_partial_success() {
        for bad in [
            ".def x",
            ".def x 2147483648",
            ".def x +1",
            ".var G x",
            ".var L0",
            ".var X0 x",
            ".var G4294967296 x",
            ".call a b",
            ".load",
            ".system \"a b\"",
            ".load a ; comment",
            ".img bg",
            "?x=1",
            "@label",
            "dialogue",
            ".call a\0",
        ] {
            let data = format!(".load title\n  {bad}");
            let err = parse_bootstrap(data.as_bytes(), 100).unwrap_err();
            assert_eq!(err.line, 2, "{bad}");
            assert_eq!(err.offset, 14, "{bad}");
        }
    }
    #[test]
    fn empty_and_unterminated_line() {
        assert!(parse_bootstrap(b"", 0).unwrap().is_empty());
        assert!(parse_bootstrap(b" \t\n;comment", 0).unwrap().is_empty());
        assert_eq!(parse_bootstrap(b".call target", 1).unwrap()[0].span, 0..12);
    }
}
