//! RFC-4180 CSV field quoting/parsing (std-only) for the dictionary and
//! snippets stores. The old store code wrote raw `field,field\n` and parsed
//! with `lines()` + `splitn(2, ',')`, which corrupted any field containing a
//! comma, double-quote, CR, or LF — multi-line snippet expansions (signature/
//! address blocks) are a real use case, so this matters.
//!
//! Privacy §10.1: CSV content is user data (entries/expansions); these helpers
//! never log it. Error mapping stays per-store (metadata-only io/sqlite).

/// RFC-4180-quote a single field: wrap in `"..."` if it contains `,` `"` `\r`
/// or `\n`; escape each internal `"` as `""`.
pub fn quote_field(field: &str) -> String {
    // RFC-4180 §2.6/§2.7: quote iff the field has `,` `"` `\r` or `\n`.
    let needs_quoting =
        field.contains(',') || field.contains('"') || field.contains('\r') || field.contains('\n');
    if !needs_quoting {
        return field.to_string();
    }
    let mut out = String::with_capacity(field.len() + 2);
    out.push('"');
    for c in field.chars() {
        // RFC-4180 §2.7: a literal `"` inside a quoted field is doubled.
        if c == '"' {
            out.push('"');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Parse RFC-4180 CSV into rows of fields. Handles quoted fields containing
/// `,`/CR/LF and `""`-escaped quotes. Accepts both CRLF (RFC-4180 canonical)
/// and bare LF row separators. A trailing newline yields no spurious empty
/// trailing row. Returns ALL rows (header handling is the caller's job).
pub fn parse_rows(content: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = content.chars().collect();
    let n = chars.len();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    // Whether the current field has begun (via a content char or an opening
    // quote). Disambiguates a leading `"` (open a quoted field) from a stray
    // quote, and lets a trailing row be detected even when it's an empty
    // quoted field (`""`).
    let mut field_started = false;
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if in_quotes {
            if c == '"' {
                // RFC-4180 §2.7: `""` inside quotes = one literal `"`;
                // a lone `"` (next char isn't `"`) closes the field.
                if i + 1 < n && chars[i + 1] == '"' {
                    field.push('"');
                    i += 2;
                } else {
                    in_quotes = false;
                    i += 1;
                }
            } else {
                // Inside quotes, commas/CR/LF are literal field content —
                // the whole reason a multi-line field survives the roundtrip.
                field.push(c);
                i += 1;
            }
        } else if c == '"' && !field_started {
            // Opening quote of a quoted field.
            in_quotes = true;
            field_started = true;
            i += 1;
        } else if c == ',' {
            row.push(std::mem::take(&mut field));
            field_started = false;
            i += 1;
        } else if c == '\n' {
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            field_started = false;
            i += 1;
        } else if c == '\r' {
            // CRLF (RFC-4180 canonical) or a lone CR: flush the row, then
            // consume a paired `\n` so it doesn't leak into the next field.
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            field_started = false;
            i += 1;
            if i < n && chars[i] == '\n' {
                i += 1;
            }
        } else {
            field.push(c);
            field_started = true;
            i += 1;
        }
    }
    // Flush a final row that had no trailing newline. `field_started` covers a
    // final empty quoted field (`""`); the field/row guards cover trailing
    // content + multi-column rows. A trailing newline already flushed above
    // leaves everything empty → no spurious empty trailing row.
    if field_started || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_plain_field_no_quoting() {
        assert_eq!(quote_field("abc"), "abc");
        assert_eq!(quote_field(""), "");
        assert_eq!(quote_field("hello world"), "hello world");
    }

    #[test]
    fn quote_field_with_comma() {
        assert_eq!(quote_field("a,b"), "\"a,b\"");
    }

    #[test]
    fn quote_field_with_double_quote() {
        // RFC-4180 §2.7: internal `"` doubled.
        assert_eq!(quote_field("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn quote_field_with_newline() {
        assert_eq!(quote_field("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn quote_field_with_cr() {
        assert_eq!(quote_field("a\rb"), "\"a\rb\"");
    }

    #[test]
    fn parse_plain_two_columns() {
        assert_eq!(
            parse_rows("a,b\nc,d\n"),
            vec![vec!["a", "b"], vec!["c", "d"]]
        );
    }

    #[test]
    fn parse_quoted_field_with_comma() {
        assert_eq!(parse_rows("a,\"b,c\",d\n"), vec![vec!["a", "b,c", "d"]]);
    }

    #[test]
    fn parse_quoted_field_with_embedded_newline() {
        // The core bug: a multi-line field must NOT be split into rows.
        assert_eq!(
            parse_rows("a,\"line1\nline2\"\n"),
            vec![vec!["a", "line1\nline2"]]
        );
    }

    #[test]
    fn parse_escaped_quote_becomes_literal() {
        assert_eq!(parse_rows("\"b\"\"c\"\n"), vec![vec!["b\"c"]]);
    }

    #[test]
    fn parse_crlf_row_endings() {
        assert_eq!(
            parse_rows("a,b\r\nc,d\r\n"),
            vec![vec!["a", "b"], vec!["c", "d"]]
        );
    }

    #[test]
    fn parse_bare_lf_row_endings() {
        assert_eq!(
            parse_rows("a,b\nc,d\n"),
            vec![vec!["a", "b"], vec!["c", "d"]]
        );
    }

    #[test]
    fn parse_trailing_newline_no_empty_row() {
        assert_eq!(parse_rows("a,b\n"), vec![vec!["a", "b"]]);
    }

    #[test]
    fn parse_final_row_without_trailing_newline() {
        assert_eq!(parse_rows("a,b\nc,d"), vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    #[test]
    fn parse_empty_quoted_field() {
        assert_eq!(parse_rows("\"\"\n"), vec![vec![""]]);
    }
}
