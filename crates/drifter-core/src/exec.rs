pub fn words(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quoted = false;
    let mut chars = line.chars();

    while let Some(c) = chars.next() {
        match (quoted, c) {
            (false, '"') => {
                quoted = true;
                started = true;
            }
            (true, '"') => quoted = false,
            (true, '\\') => word.push(chars.next()?),
            (false, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (_, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return None;
    }
    if started {
        words.push(word);
    }
    Some(words)
}

pub fn command(line: &str, files: &[&str]) -> Option<Vec<String>> {
    let mut argv = Vec::new();
    let mut placed = false;
    for word in words(line)? {
        match word.as_str() {
            "%F" | "%U" => {
                argv.extend(files.iter().map(|file| file.to_string()));
                placed = true;
                continue;
            }
            "%f" | "%u" => {
                argv.extend(files.first().map(|file| file.to_string()));
                placed = true;
                continue;
            }
            "%i" | "%c" | "%k" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => continue,
            _ => {}
        }

        let mut out = String::new();
        let mut chars = word.chars();
        while let Some(c) = chars.next() {
            if c != '%' {
                out.push(c);
                continue;
            }
            match chars.next() {
                Some('%') => out.push('%'),
                Some('f' | 'u') => {
                    if let Some(file) = files.first() {
                        out.push_str(file);
                    }
                    placed = true;
                }
                _ => {}
            }
        }
        argv.push(out);
    }
    if argv.is_empty() {
        return None;
    }
    if !placed {
        argv.extend(files.iter().map(|file| file.to_string()));
    }
    Some(argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> Vec<String> {
        command(text, &[]).unwrap()
    }

    #[test]
    fn a_plain_line_splits_on_spaces() {
        assert_eq!(
            line("firefox --new-window"),
            vec!["firefox", "--new-window"]
        );
    }

    #[test]
    fn a_quoted_program_path_keeps_its_spaces() {
        assert_eq!(
            line("\"/opt/My App/bin/app\" --flag %U"),
            vec!["/opt/My App/bin/app", "--flag"]
        );
    }

    #[test]
    fn escapes_inside_quotes_are_read() {
        assert_eq!(
            line(r#"sh -c "echo \"hi\" \$HOME \\ done""#),
            vec!["sh", "-c", r#"echo "hi" $HOME \ done"#]
        );
    }

    #[test]
    fn field_codes_inside_a_word_are_removed_or_filled() {
        assert_eq!(
            line("app --open=%u --name=%c"),
            vec!["app", "--open=", "--name="]
        );
        assert_eq!(
            command("app --open=%u", &["/tmp/x"]).unwrap(),
            vec!["app", "--open=/tmp/x"]
        );
        assert_eq!(line("app 100%%"), vec!["app", "100%"]);
    }

    #[test]
    fn a_folder_goes_where_the_line_asks_for_it() {
        assert_eq!(
            command("nautilus --new-window %U", &["/home/me/Trash files"]).unwrap(),
            vec!["nautilus", "--new-window", "/home/me/Trash files"]
        );
    }

    #[test]
    fn a_folder_is_added_when_the_line_has_no_place_for_it() {
        assert_eq!(
            command("thunar", &["/tmp/a b"]).unwrap(),
            vec!["thunar", "/tmp/a b"]
        );
    }

    #[test]
    fn an_open_quote_is_refused() {
        assert_eq!(command("\"/opt/app", &[]), None);
        assert_eq!(command("   ", &[]), None);
    }
}
