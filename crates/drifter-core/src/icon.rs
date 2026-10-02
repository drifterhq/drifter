use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const DEEPEST: usize = 4;
const MOST_THEMES: usize = 8;

pub fn roots() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut roots = Vec::new();
    match std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        Some(data) if data.is_absolute() => roots.push(data.join("icons")),
        _ => {
            if let Some(home) = &home {
                roots.push(home.join(".local/share/icons"));
            }
        }
    }
    if let Some(home) = &home {
        roots.push(home.join(".icons"));
    }
    let shared = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|dirs| !dirs.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".to_string());
    for dir in shared.split(':').filter(|dir| dir.starts_with('/')) {
        roots.push(Path::new(dir).join("icons"));
    }
    roots
}

pub fn chosen_theme() -> Option<String> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    ["gtk-4.0/settings.ini", "gtk-3.0/settings.ini"]
        .iter()
        .filter_map(|file| std::fs::read_to_string(config.join(file)).ok())
        .find_map(|text| setting(&text, "gtk-icon-theme-name"))
        .or_else(gnome_theme)
        .or_else(|| Some(GTK_DEFAULT.to_string()))
}

const GTK_DEFAULT: &str = "Adwaita";

fn gnome_theme() -> Option<String> {
    let out = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "icon-theme"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let name = text.trim().trim_matches('\'').to_string();
    (!name.is_empty()).then_some(name)
}

pub fn is_symbolic(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.ends_with("-symbolic"))
}

fn setting(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key)
            .then(|| value.trim().trim_matches('"').to_string())
            .filter(|value| !value.is_empty())
    })
}

pub fn themes(first: Option<&str>, roots: &[PathBuf]) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut waiting: Vec<String> = first.map(str::to_string).into_iter().collect();
    while let Some(theme) = waiting.pop() {
        if order.contains(&theme) || order.len() >= MOST_THEMES {
            continue;
        }
        let parents = roots
            .iter()
            .filter_map(|root| std::fs::read_to_string(root.join(&theme).join("index.theme")).ok())
            .find_map(|text| setting(&text, "Inherits"))
            .unwrap_or_default();
        order.push(theme);
        for parent in parents.split(',').rev() {
            let parent = parent.trim();
            if !parent.is_empty() {
                waiting.push(parent.to_string());
            }
        }
    }
    order.retain(|theme| theme != "hicolor");
    order.push("hicolor".to_string());
    order
}

fn size_of(path: &Path) -> Option<u32> {
    path.components().rev().find_map(|part| {
        let part = part.as_os_str().to_str()?;
        let lead = part.split(['x', '@']).next()?;
        lead.parse().ok()
    })
}

fn score(path: &Path, want: u32) -> u32 {
    let svg = path.extension().is_some_and(|ext| ext == "svg");
    match size_of(path) {
        Some(size) if size >= want => (size - want) * 2,
        Some(size) => (want - size) * 4,
        None if svg => 1,
        None => u32::MAX / 2,
    }
}

fn gather(dir: &Path, file: &str, depth: usize, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() || (kind.is_symlink() && path.is_dir()) {
            if depth < DEEPEST {
                gather(&path, file, depth + 1, found);
            }
        } else if entry.file_name().to_str() == Some(file) {
            found.push(path);
        }
    }
}

fn best(dir: &Path, name: &str, want: u32) -> Option<PathBuf> {
    let mut found = Vec::new();
    gather(dir, &format!("{name}.png"), 0, &mut found);
    gather(dir, &format!("{name}.svg"), 0, &mut found);
    found.sort();
    found
        .into_iter()
        .min_by_key(|path| score(path.strip_prefix(dir).unwrap_or(path), want))
}

pub fn find_in(
    name: &str,
    extra: Option<&Path>,
    roots: &[PathBuf],
    themes: &[String],
    want: u32,
) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let direct = Path::new(name);
    if direct.is_absolute() {
        return direct.is_file().then(|| direct.to_path_buf());
    }
    if let Some(found) = extra.and_then(|dir| best(dir, name, want)) {
        return Some(found);
    }
    for theme in themes {
        let found = roots
            .iter()
            .filter_map(|root| {
                let dir = root.join(theme);
                best(&dir, name, want).map(|path| {
                    let fit = score(path.strip_prefix(&dir).unwrap_or(&path), want);
                    (fit, path)
                })
            })
            .min_by_key(|(fit, _)| *fit)
            .map(|(_, path)| path);
        if found.is_some() {
            return found;
        }
    }
    let folders: Vec<PathBuf> = std::iter::once(PathBuf::from("/usr/share/pixmaps"))
        .chain(
            roots
                .iter()
                .filter_map(|root| root.parent())
                .map(|share| share.join("pixmaps")),
        )
        .collect();
    ["png", "svg"].iter().find_map(|ext| {
        folders
            .iter()
            .map(|folder| folder.join(format!("{name}.{ext}")))
            .find(|path| path.is_file())
    })
}

type Asked = (String, Option<PathBuf>, u32);

static SEEN: Mutex<Option<HashMap<Asked, Option<PathBuf>>>> = Mutex::new(None);

pub fn find(name: &str, extra: Option<&Path>, want: u32) -> Option<PathBuf> {
    let key = (name.to_string(), extra.map(Path::to_path_buf), want);
    if let Some(known) = SEEN
        .lock()
        .ok()
        .and_then(|seen| seen.as_ref().and_then(|seen| seen.get(&key).cloned()))
    {
        return known;
    }
    let roots = roots();
    let themes = themes(chosen_theme().as_deref(), &roots);
    let found = find_in(name, extra, &roots, &themes, want).or_else(|| {
        (!name.ends_with("-symbolic"))
            .then(|| find_in(&format!("{name}-symbolic"), extra, &roots, &themes, want))
            .flatten()
    });
    if let Ok(mut seen) = SEEN.lock() {
        seen.get_or_insert_with(HashMap::new)
            .insert(key, found.clone());
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("drifter-icon-tests").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn reads_the_icon_theme_from_gtk_settings() {
        let text = "[Settings]\ngtk-theme-name=Adwaita\ngtk-icon-theme-name = Papirus-Dark\n";
        assert_eq!(
            setting(text, "gtk-icon-theme-name").as_deref(),
            Some("Papirus-Dark")
        );
        assert_eq!(setting("[Settings]\n", "gtk-icon-theme-name"), None);
    }

    #[test]
    fn a_theme_brings_its_parents_and_hicolor_comes_last() {
        let root = scratch("inherit");
        std::fs::create_dir_all(root.join("Child")).unwrap();
        std::fs::write(
            root.join("Child/index.theme"),
            "[Icon Theme]\nInherits=Parent,hicolor\n",
        )
        .unwrap();
        let order = themes(Some("Child"), &[root]);
        assert_eq!(order, vec!["Child", "Parent", "hicolor"]);
    }

    #[test]
    fn a_theme_that_inherits_itself_does_not_loop() {
        let root = scratch("loop");
        std::fs::create_dir_all(root.join("Loop")).unwrap();
        std::fs::write(root.join("Loop/index.theme"), "Inherits=Loop\n").unwrap();
        assert_eq!(themes(Some("Loop"), &[root]), vec!["Loop", "hicolor"]);
    }

    #[test]
    fn finds_an_icon_in_the_users_theme_before_hicolor() {
        let root = scratch("user");
        touch(&root.join("Mine/24x24/panel/nm-signal.png"));
        touch(&root.join("hicolor/24x24/apps/nm-signal.png"));
        let themes = vec!["Mine".to_string(), "hicolor".to_string()];
        let found = find_in("nm-signal", None, std::slice::from_ref(&root), &themes, 24).unwrap();
        assert!(found.starts_with(root.join("Mine")));
    }

    #[test]
    fn picks_the_size_closest_to_the_wanted_one() {
        let root = scratch("sizes");
        touch(&root.join("hicolor/16x16/apps/tool.png"));
        touch(&root.join("hicolor/32x32/apps/tool.png"));
        touch(&root.join("hicolor/256x256/apps/tool.png"));
        touch(&root.join("hicolor/apps/48/tool.png"));
        let themes = vec!["hicolor".to_string()];
        let found = find_in("tool", None, std::slice::from_ref(&root), &themes, 24).unwrap();
        assert!(
            found.ends_with("32x32/apps/tool.png"),
            "{}",
            found.display()
        );
        let big = find_in("tool", None, &[root], &themes, 48).unwrap();
        assert!(big.ends_with("apps/48/tool.png"), "{}", big.display());
    }

    #[test]
    fn the_items_own_theme_path_wins() {
        let root = scratch("own");
        touch(&root.join("hicolor/24x24/apps/app.png"));
        let own = scratch("own-path");
        touch(&own.join("app.png"));
        let themes = vec!["hicolor".to_string()];
        let found = find_in("app", Some(&own), &[root], &themes, 24).unwrap();
        assert_eq!(found, own.join("app.png"));
    }

    #[test]
    fn a_scalable_svg_beats_a_png_of_the_wrong_size() {
        let root = scratch("svg");
        touch(&root.join("Theme/48x48/apps/tool.png"));
        touch(&root.join("Theme/scalable/apps/tool.svg"));
        touch(&root.join("Theme/24x24/apps/exact.png"));
        touch(&root.join("Theme/scalable/apps/exact.svg"));
        let themes = vec!["Theme".to_string()];
        let found = find_in("tool", None, &[root.clone()], &themes, 24).unwrap();
        assert!(found.ends_with("scalable/apps/tool.svg"));
        let found = find_in("exact", None, &[root], &themes, 24).unwrap();
        assert!(found.ends_with("24x24/apps/exact.png"));
    }

    #[test]
    fn a_symbolic_icon_is_known_by_its_name() {
        assert!(is_symbolic(Path::new("/x/network-wireless-symbolic.svg")));
        assert!(!is_symbolic(Path::new("/x/network-wireless.svg")));
    }

    #[test]
    fn a_missing_icon_is_none() {
        let root = scratch("empty");
        assert_eq!(
            find_in("nothing-here", None, &[root], &["hicolor".to_string()], 24),
            None
        );
    }
}
