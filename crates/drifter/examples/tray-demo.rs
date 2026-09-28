const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";

struct Item {
    id: String,
    pixmap: Vec<u8>,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    fn activate(&self, x: i32, y: i32) {
        println!("{}: left click at {x} {y}", self.id);
    }

    fn context_menu(&self, x: i32, y: i32) {
        println!("{}: right click at {x} {y}", self.id);
    }

    fn secondary_activate(&self, x: i32, y: i32) {
        println!("{}: middle click at {x} {y}", self.id);
    }

    #[zbus(property)]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn title(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn category(&self) -> String {
        "ApplicationStatus".to_string()
    }

    #[zbus(property)]
    fn status(&self) -> String {
        "Active".to_string()
    }

    #[zbus(property)]
    fn icon_name(&self) -> String {
        String::new()
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![(24, 24, self.pixmap.clone())]
    }
}

fn square(size: i32, color: [u8; 4]) -> Vec<u8> {
    let mut argb = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let edge = x < 3 || y < 3 || x >= size - 3 || y >= size - 3;
            let middle =
                x >= size / 4 && x < size - size / 4 && y >= size / 4 && y < size - size / 4;
            if edge || middle {
                argb.extend_from_slice(&color);
            } else {
                argb.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    argb
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let items = [
        ("/StatusNotifierItem", "drifter-demo-a", [255, 90, 200, 255]),
        (
            "/StatusNotifierItem2",
            "drifter-demo-b",
            [255, 255, 190, 90],
        ),
    ];

    let mut builder = zbus::blocking::connection::Builder::session()?
        .name(format!("org.drifter.TrayDemo{}", std::process::id()))?;

    for (path, id, color) in items {
        builder = builder.serve_at(
            path,
            Item {
                id: id.to_string(),
                pixmap: square(24, color),
            },
        )?;
    }

    let connection = builder.build()?;

    let watcher = zbus::blocking::Proxy::new(&connection, WATCHER, WATCHER_PATH, WATCHER)?;
    for (path, id, _) in items {
        watcher.call_method("RegisterStatusNotifierItem", &(path))?;
        println!("registered {id} at {path}");
    }

    println!("waiting for clicks. ctrl-c to stop.");
    loop {
        std::thread::park();
    }
}
