use drifter_core::power::Action;

const SERVICE: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";

pub fn run(action: Action) {
    let Some(method) = action.method() else {
        return;
    };
    std::thread::spawn(move || {
        if action == Action::Suspend {
            std::thread::sleep(std::time::Duration::from_millis(400));
        }
        if let Err(error) = call(method) {
            log!("drifter: {} failed: {error}", action.label());
        }
    });
}

fn call(method: &str) -> Result<(), String> {
    let connection = zbus::blocking::Connection::system().map_err(|e| e.to_string())?;
    let proxy = zbus::blocking::Proxy::new(&connection, SERVICE, PATH, MANAGER)
        .map_err(|e| e.to_string())?;
    proxy
        .call_method(method, &(false,))
        .map_err(|e| e.to_string())?;
    Ok(())
}
