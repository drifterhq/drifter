fn main() {
    println!("current_exe = {:?}", std::env::current_exe());
    println!("terminal_app = {:?}", drifter_core::desktop::terminal_app().map(|a| (a.id, a.name)));
    println!("-- terminals in installed() --");
    for app in drifter_core::desktop::installed() {
        if app.categories.iter().any(|c| c == "TerminalEmulator") || app.id.contains("terminal") {
            println!("  id={} name={} icon={:?}", app.id, app.name, app.icon);
        }
    }
    println!("system_default(Terminal) = {:?}", drifter_core::desktop::system_default(drifter_core::desktop::Role::Terminal));
}
