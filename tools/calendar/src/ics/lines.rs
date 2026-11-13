//! Splits an .ics file into properties, joining folded lines and unescaping text.

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub value: String,
}

impl Property {
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(name, _)| name.eq_ignore_ascii_case(key)).map(|(_, value)| value.as_str())
    }

    pub fn text(&self) -> String {
        unescape(&self.value)
    }
}

pub fn properties(text: &str) -> Vec<Property> {
    unfold(text).iter().filter_map(|line| property(line)).collect()
}

fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        match (line.strip_prefix([' ', '\t']), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ if line.is_empty() => {}
            _ => lines.push(line.to_owned()),
        }
    }
    lines
}

fn property(line: &str) -> Option<Property> {
    let colon = outside_quotes(line, ':')?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = split_outside_quotes(head, ';').into_iter();
    let name = parts.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = parts
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((key.trim().to_ascii_uppercase(), value.trim().trim_matches('"').to_owned()))
        })
        .collect();
    Some(Property { name, params, value: value.to_owned() })
}

fn outside_quotes(text: &str, wanted: char) -> Option<usize> {
    let mut quoted = false;
    text.char_indices().find_map(|(at, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        (c == wanted && !quoted).then_some(at)
    })
}

fn split_outside_quotes(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(at) = outside_quotes(rest, separator) {
        parts.push(&rest[..at]);
        rest = &rest[at + separator.len_utf8()..];
    }
    parts.push(rest);
    parts
}

pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_lines_are_joined() {
        let text = "BEGIN:VEVENT\r\nDESCRIPTION:Join at https://meet.goo\r\n gle.com/abc-defg-hij\r\nDTSTART;TZID=\"America/New_York\":20260930T090000\r\nEND:VEVENT\r\n";
        let props = properties(text);
        assert_eq!(props.len(), 4);
        assert_eq!(props[1].value, "Join at https://meet.google.com/abc-defg-hij");
        assert_eq!(props[2].name, "DTSTART");
        assert_eq!(props[2].param("tzid"), Some("America/New_York"));
        assert_eq!(props[2].value, "20260930T090000");
    }

    #[test]
    fn quoted_colon_is_not_the_value() {
        let props = properties("ATTENDEE;CN=\"Doe: Jane\";PARTSTAT=DECLINED:mailto:jane@example.com");
        assert_eq!(props[0].param("CN"), Some("Doe: Jane"));
        assert_eq!(props[0].param("partstat"), Some("DECLINED"));
        assert_eq!(props[0].value, "mailto:jane@example.com");
    }

    #[test]
    fn text_values_are_unescaped() {
        assert_eq!(
            unescape(r"Room 4\, floor 2\nBring notes\; coffee\\tea"),
            "Room 4, floor 2\nBring notes; coffee\\tea"
        );
    }
}
