//! Mixed-encoding source overlays retain original byte literals and replace
//! translated literals. This is the verified PK02 text profile, not a property
//! of the archive format itself.
use std::{collections::BTreeSet, ops::Range};

fn decode(raw: &[u8], encoding: &'static encoding_rs::Encoding) -> Result<String, String> {
    let (text, errors) = encoding.decode_without_bom_handling(raw);
    if errors {
        return Err(format!("invalid {} patch text", encoding.name()));
    }
    Ok(text.into_owned())
}

// Scan byte source before Unicode conversion. A DBCS trailing backslash must
// not escape the closing quote. Comments never contribute literal provenance.
fn literals(raw: &[u8]) -> Result<Vec<Range<usize>>, String> {
    let mut result = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        if raw[i..].starts_with(b"//") {
            while i < raw.len() && raw[i] != b'\n' {
                i += 1;
            }
        } else if raw[i..].starts_with(b"/*") {
            i += 2;
            while i < raw.len() && !raw[i..].starts_with(b"*/") {
                i += 1;
            }
            if i == raw.len() {
                return Err("unterminated patch source comment".into());
            }
            i += 2;
        } else if matches!(raw[i], b'\'' | b'"') {
            let start = i;
            let quote = raw[i];
            let verbatim = i > 0 && raw[i - 1] == b'@';
            i += 1;
            loop {
                if i >= raw.len() {
                    return Err("unterminated patch source literal".into());
                }
                if raw[i] == quote {
                    if verbatim && raw.get(i + 1) == Some(&quote) {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    result.push(start..i);
                    break;
                }
                if (raw[i] == b'\\' && !verbatim)
                    || (raw[i] >= 0x81 && raw.get(i + 1).is_some_and(|b| *b >= 0x40 && *b != 0x7f))
                {
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }
    Ok(result)
}

pub(crate) fn system(raw: &[u8], original: &[u8]) -> Result<String, String> {
    if original.is_empty() && !raw.is_ascii() {
        return Err("mixed PK02 system source requires original encoding provenance".into());
    }
    let original_literals: BTreeSet<_> = literals(original)?
        .into_iter()
        .map(|r| &original[r])
        .collect();
    let mut out = String::new();
    let mut end = 0;
    for range in literals(raw)? {
        out.push_str(&decode(&raw[end..range.start], encoding_rs::SHIFT_JIS)?);
        let literal = &raw[range.clone()];
        let encoding = if original_literals.contains(literal) {
            encoding_rs::SHIFT_JIS
        } else {
            encoding_rs::GBK
        };
        out.push_str(&decode(literal, encoding)?);
        end = range.end;
    }
    out.push_str(&decode(&raw[end..], encoding_rs::SHIFT_JIS)?);
    Ok(out)
}

pub(crate) fn data(raw: &[u8], original: &[u8]) -> Result<String, String> {
    if original.is_empty() && !raw.is_ascii() {
        return Err("mixed PK02 data source requires original encoding provenance".into());
    }
    let original_tokens: BTreeSet<_> = original.split(|b| b.is_ascii_whitespace()).collect();
    let mut out = String::new();
    for line in raw.split_inclusive(|b| *b == b'\n') {
        if line.trim_ascii_start().starts_with(b";") {
            out.push('\n');
            continue;
        }
        let mut at = 0;
        while at < line.len() {
            if line[at].is_ascii_whitespace() {
                out.push(char::from(line[at]));
                at += 1;
                continue;
            }
            let start = at;
            while at < line.len() && !line[at].is_ascii_whitespace() {
                at += 1;
            }
            let token = &line[start..at];
            out.push_str(&decode(
                token,
                if original_tokens.contains(token) {
                    encoding_rs::SHIFT_JIS
                } else {
                    encoding_rs::GBK
                },
            )?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_keys_and_translated_literals_keep_distinct_encodings() {
        let original = encoding_rs::SHIFT_JIS
            .encode("// 注釈\nkey <- \"ソラ\"; caption <- \"日本語\"; local x='「';")
            .0
            .into_owned();
        let prefix = encoding_rs::SHIFT_JIS
            .encode("// 注釈\nkey <- \"ソラ\"; caption <- ")
            .0
            .into_owned();
        let translated = encoding_rs::GBK
            .encode("\"中文\"; local x='「';")
            .0
            .into_owned();
        assert_eq!(
            system(&[prefix, translated].concat(), &original).unwrap(),
            "// 注釈\nkey <- \"ソラ\"; caption <- \"中文\"; local x='「';"
        );
        assert!(system(b"local x=\"unterminated", b"").is_err());
    }
    #[test]
    fn data_keeps_original_resource_keys_and_decodes_new_labels() {
        let original = encoding_rs::SHIFT_JIS
            .encode("曲.ogg 日本語\n")
            .0
            .into_owned();
        let raw = [
            encoding_rs::SHIFT_JIS.encode("曲.ogg ").0.into_owned(),
            encoding_rs::GBK.encode("中文\n").0.into_owned(),
        ]
        .concat();
        assert_eq!(data(&raw, &original).unwrap(), "曲.ogg 中文\n");
    }
}
