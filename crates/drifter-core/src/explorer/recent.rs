use std::path::{Path, PathBuf};

pub const MOST_SHOWN: usize = 6;

pub fn list_path(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
        .join("recently-used.xbel")
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

fn unescape(text: &str) -> String {
    text.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'")
}

pub fn parse(text: &str) -> Vec<(PathBuf, String)> {
    let mut found: Vec<(PathBuf, String)> = Vec::new();
    for piece in text.split("<bookmark").skip(1) {
        let Some(tag) = piece.split('>').next() else {
            continue;
        };
        let Some(href) = attribute(tag, "href") else {
            continue;
        };
        let Some(rest) = unescape(href).strip_prefix("file://").map(str::to_string) else {
            continue;
        };
        let when = attribute(tag, "visited").or_else(|| attribute(tag, "modified")).or_else(|| attribute(tag, "added")).unwrap_or("").to_string();
        found.push((PathBuf::from(super::ops::decode(&rest)), when));
    }
    found.sort_by(|a, b| b.1.cmp(&a.1));
    found
}

const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xbel version=\"1.0\"\n      xmlns:bookmark=\"http://www.freedesktop.org/standards/desktop-bookmarks\"\n      xmlns:mime=\"http://www.freedesktop.org/standards/shared-mime-info\"\n>\n";
const MOST_KEPT: usize = 500;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

fn set_attribute(tag: &str, name: &str, value: &str) -> String {
    match tag.find(&format!(" {name}=\"")) {
        Some(at) => {
            let start = at + name.len() + 3;
            let end = tag[start..].find('"').map_or(tag.len(), |end| start + end);
            format!("{}{value}{}", &tag[..start], &tag[end..])
        }
        None => format!("{tag} {name}=\"{value}\""),
    }
}

pub fn touched(text: &str, opened: &[(PathBuf, &str)], now: &str) -> String {
    let mut text = match text.contains("</xbel>") {
        true => text.to_string(),
        false => format!("{HEAD}</xbel>\n"),
    };
    for (path, mime) in opened {
        let href = format!("file://{}", super::ops::encode(path));
        let marker = format!("<bookmark href=\"{}\"", escape(&href));
        match text.find(&marker) {
            Some(at) => {
                let end = text[at..].find('>').map_or(text.len(), |end| at + end);
                let closing = if text[..end].ends_with('/') { end - 1 } else { end };
                let tag = &text[at..closing];
                let tag = set_attribute(&set_attribute(tag, "modified", now), "visited", now);
                text.replace_range(at..closing, &tag);
            }
            None => {
                let entry = format!(
                    "  <bookmark href=\"{}\" added=\"{now}\" modified=\"{now}\" visited=\"{now}\">\n    <info>\n      <metadata owner=\"http://freedesktop.org\">\n        <mime:mime-type type=\"{}\"/>\n        <bookmark:applications>\n          <bookmark:application name=\"Explorer\" exec=\"&apos;drifter-explorer %u&apos;\" modified=\"{now}\" count=\"1\"/>\n        </bookmark:applications>\n      </metadata>\n    </info>\n  </bookmark>\n",
                    escape(&href),
                    escape(mime),
                );
                let at = text.rfind("</xbel>").unwrap_or(text.len());
                text.insert_str(at, &entry);
            }
        }
    }
    let count = text.matches("<bookmark ").count();
    if count > MOST_KEPT {
        let mut drop = count - MOST_KEPT;
        while drop > 0 {
            let Some(start) = text.find("<bookmark ") else {
                break;
            };
            let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
            let tag_end = text[start..].find('>').map_or(text.len(), |end| start + end + 1);
            let end = match text[start..tag_end].ends_with("/>") {
                true => tag_end,
                false => text[start..].find("</bookmark>").map_or(text.len(), |end| start + end + "</bookmark>".len()),
            };
            let end = if text[end..].starts_with('\n') { end + 1 } else { end };
            text.replace_range(line_start..end, "");
            drop -= 1;
        }
    }
    text
}

pub fn remember(path: &Path, opened: &[(PathBuf, &str)]) -> std::io::Result<()> {
    if opened.is_empty() {
        return Ok(());
    }
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string();
    let out = touched(&text, opened, &now);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let spare = path.with_extension("xbel.drifter");
    std::fs::write(&spare, out)?;
    std::fs::rename(&spare, path)
}

pub fn load(list: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(list) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for (path, _) in parse(&text) {
        let hidden = path.components().any(|part| part.as_os_str().to_string_lossy().starts_with('.'));
        if hidden || super::ops::in_trash(&path) || !path.is_file() || out.contains(&path) {
            continue;
        }
        out.push(path);
        if out.len() == MOST_SHOWN {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_files_come_first_and_odd_entries_are_skipped() {
        let text = r#"<?xml version="1.0"?>
<xbel version="1.0">
  <bookmark href="file:///tmp/old.txt" added="2026-01-01T00:00:00Z" modified="2026-01-01T00:00:00Z" visited="2026-01-01T00:00:00Z">
  </bookmark>
  <bookmark href="file:///tmp/My%20Notes%20&amp;%20more.txt" added="2026-02-01T00:00:00Z" modified="2026-03-01T00:00:00Z" visited="2026-03-01T00:00:00Z"/>
  <bookmark href="https://example.com/page" added="2026-04-01T00:00:00Z"/>
</xbel>"#;
        let found: Vec<PathBuf> = parse(text).into_iter().map(|(path, _)| path).collect();
        assert_eq!(found, [PathBuf::from("/tmp/My Notes & more.txt"), PathBuf::from("/tmp/old.txt")]);
    }

    #[test]
    fn opening_a_file_adds_it_or_moves_it_to_the_top() {
        let fresh = touched("", &[(PathBuf::from("/tmp/a b.txt"), "text/plain")], "2026-10-10T10:00:00Z");
        assert!(fresh.starts_with("<?xml") && fresh.trim_end().ends_with("</xbel>"));
        assert!(fresh.contains("href=\"file:///tmp/a%20b.txt\"") && fresh.contains("type=\"text/plain\""));
        let later = touched(&fresh, &[(PathBuf::from("/tmp/c.txt"), "text/plain")], "2026-10-10T11:00:00Z");
        let again = touched(&later, &[(PathBuf::from("/tmp/a b.txt"), "text/plain")], "2026-10-10T12:00:00Z");
        assert_eq!(again.matches("<bookmark ").count(), 2, "no second entry for the same file");
        let order: Vec<PathBuf> = parse(&again).into_iter().map(|(path, _)| path).collect();
        assert_eq!(order, [PathBuf::from("/tmp/a b.txt"), PathBuf::from("/tmp/c.txt")]);
        assert!(again.contains("added=\"2026-10-10T10:00:00Z\""), "the first time stays");
        let full = (0..MOST_KEPT + 3).fold(String::new(), |text, at| touched(&text, &[(PathBuf::from(format!("/tmp/{at}.txt")), "text/plain")], &format!("2026-10-10T10:{:02}:{:02}Z", at / 60, at % 60)));
        assert_eq!(full.matches("<bookmark ").count(), MOST_KEPT);
        assert!(!full.contains("/tmp/0.txt\"") && full.contains(&format!("/tmp/{}.txt", MOST_KEPT + 2)));
    }
}
