use std::path::PathBuf;

use smithay::reexports::calloop::channel::{Channel, Sender as LoopSender, channel as loop_channel};

const NAME: &str = "org.freedesktop.FileManager1";
const PATH: &str = "/org/freedesktop/FileManager1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Show {
    Folders(Vec<PathBuf>),
    Items(Vec<PathBuf>),
}

pub fn start() -> Result<Channel<Show>, String> {
    let (shows, shown) = loop_channel::<Show>();
    std::thread::Builder::new()
        .name("drifter-filemanager".to_string())
        .spawn(move || serve(shows))
        .map_err(|e| format!("cannot start the file manager service: {e}"))?;
    Ok(shown)
}

fn serve(shows: LoopSender<Show>) {
    let address = match crate::tray::bus_address() {
        Ok(address) => address,
        Err(error) => {
            log!("drifter: no file manager service: {error}");
            return;
        }
    };
    let built = zbus::blocking::connection::Builder::address(address.as_str())
        .and_then(|builder| builder.name(NAME))
        .and_then(|builder| builder.serve_at(PATH, Server { shows }))
        .and_then(|builder| builder.build());
    let _connection = match built {
        Ok(connection) => connection,
        Err(error) => {
            log!("drifter: no file manager service: {error}");
            return;
        }
    };
    loop {
        std::thread::park();
    }
}

fn paths(uris: &[String]) -> Vec<PathBuf> {
    uris.iter()
        .filter_map(|uri| match uri.starts_with("file://") {
            true => drifter_core::explorer::clip::file_path(uri),
            false => uri.starts_with('/').then(|| PathBuf::from(uri)),
        })
        .collect()
}

struct Server {
    shows: LoopSender<Show>,
}

#[zbus::interface(name = "org.freedesktop.FileManager1")]
impl Server {
    fn show_folders(&self, uris: Vec<String>, _startup_id: String) {
        let _ = self.shows.send(Show::Folders(paths(&uris)));
    }

    fn show_items(&self, uris: Vec<String>, _startup_id: String) {
        let _ = self.shows.send(Show::Items(paths(&uris)));
    }

    fn show_item_properties(&self, uris: Vec<String>, _startup_id: String) {
        let _ = self.shows.send(Show::Items(paths(&uris)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_and_plain_paths_become_paths() {
        let got = paths(&[
            "file:///home/me/Downloads/My%20File.zip".to_string(),
            "/home/me/notes.txt".to_string(),
            "https://example.com/a".to_string(),
        ]);
        assert_eq!(got, vec![PathBuf::from("/home/me/Downloads/My File.zip"), PathBuf::from("/home/me/notes.txt")]);
    }
}
