use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handler {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
}

fn parse(id: &str, text: &str) -> Option<(Handler, Vec<String>)> {
    let app = crate::desktop::read(id, &text.replace("NoDisplay=true", ""))?;
    let exec = app.exec.clone()?;
    let mut inside = false;
    let mut mimes = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
            continue;
        }
        if inside && let Some(list) = line.strip_prefix("MimeType=") {
            mimes.extend(list.split(';').map(str::trim).filter(|mime| !mime.is_empty()).map(str::to_string));
        }
    }
    Some((Handler { id: app.id, name: app.name, exec, icon: app.icon }, mimes))
}

fn wanted(mime: &str) -> Vec<String> {
    let mut out = vec![mime.to_string()];
    if mime.starts_with("text/") || matches!(mime, "application/json" | "application/toml" | "application/x-yaml" | "application/xml" | "application/x-shellscript") {
        out.push("text/plain".to_string());
    }
    out
}

pub fn handlers_in(mime: &str, roots: &[PathBuf], lists: &[PathBuf]) -> Vec<Handler> {
    let wanted = wanted(mime);
    let mut found: Vec<(Handler, Vec<String>)> = Vec::new();
    for root in roots {
        let Ok(items) = std::fs::read_dir(root) else {
            continue;
        };
        for item in items.flatten() {
            let path = item.path();
            let Some(id) = path.file_name().and_then(|name| name.to_str()).and_then(|name| name.strip_suffix(".desktop")) else {
                continue;
            };
            if found.iter().any(|(each, _)| each.id == id) {
                continue;
            }
            if let Some(parsed) = std::fs::read_to_string(&path).ok().and_then(|text| parse(id, &text)) {
                found.push(parsed);
            }
        }
    }
    let mut chosen: Vec<String> = Vec::new();
    for list in lists {
        if let Ok(text) = std::fs::read_to_string(list) {
            for mime in &wanted {
                chosen.extend(crate::desktop::chosen_in(&text, mime));
            }
        }
    }
    let mut out: Vec<Handler> = Vec::new();
    for id in &chosen {
        if let Some((handler, _)) = found.iter().find(|(each, _)| &each.id == id)
            && !out.iter().any(|each| each.id == handler.id)
        {
            out.push(handler.clone());
        }
    }
    let mut rest: Vec<Handler> = found
        .iter()
        .filter(|(_, mimes)| mimes.iter().any(|each| wanted.contains(each)))
        .map(|(handler, _)| handler.clone())
        .filter(|handler| !out.iter().any(|each| each.id == handler.id))
        .collect();
    rest.sort_by(|a, b| crate::explorer::entry::natural(&a.name, &b.name));
    out.extend(rest);
    out
}

pub fn handlers(mime: &str) -> Vec<Handler> {
    handlers_in(mime, &crate::desktop::roots(), &crate::desktop::mime_lists())
}

pub fn argv(handler: &Handler, paths: &[&Path]) -> Option<Vec<String>> {
    let files: Vec<String> = paths.iter().map(|path| path.to_string_lossy().into_owned()).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    crate::exec::command(&handler.exec, &files)
}

pub fn fallback(path: &Path) -> Vec<String> {
    vec!["xdg-open".to_string(), path.to_string_lossy().into_owned()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_app_comes_first_then_the_rest() {
        let dir = std::env::temp_dir().join(format!("drifter-open-{}", std::process::id()));
        let apps = dir.join("applications");
        std::fs::create_dir_all(&apps).unwrap();
        let entry = |name: &str, mimes: &str| format!("[Desktop Entry]\nType=Application\nName={name}\nExec={name} %F\nMimeType={mimes}\n");
        std::fs::write(apps.join("viewer.desktop"), entry("Viewer", "image/png;image/jpeg;")).unwrap();
        std::fs::write(apps.join("art.desktop"), entry("Art", "image/png;")).unwrap();
        std::fs::write(apps.join("edit.desktop"), entry("Edit", "text/plain;")).unwrap();
        let list = dir.join("mimeapps.list");
        std::fs::write(&list, "[Default Applications]\nimage/png=viewer.desktop\n").unwrap();
        let found = handlers_in("image/png", std::slice::from_ref(&apps), std::slice::from_ref(&list));
        assert_eq!(found.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["Viewer", "Art"]);
        let code = handlers_in("text/x-rust", &[apps], &[list]);
        assert_eq!(code.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["Edit"]);
        let line = argv(&found[0], &[Path::new("/a b.png")]).unwrap();
        assert_eq!(line, ["Viewer", "/a b.png"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
