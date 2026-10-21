use std::process::{Command, Stdio};

const ERTM: &str = "/sys/module/bluetooth/parameters/disable_ertm";
const KEPT: &str = "/etc/modprobe.d/drifter-xbox.conf";

pub fn ertm_on() -> bool {
    std::fs::read_to_string(ERTM).is_ok_and(|text| text.trim() == "N")
}

pub fn fix_ertm() -> Result<(), String> {
    let script = format!("echo Y > {ERTM} && echo 'options bluetooth disable_ertm=Y' > {KEPT}");
    let out = Command::new("pkexec")
        .args(["sh", "-c", &script])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    match out.status.success() {
        true => Ok(()),
        false => Err(drifter_core::lang::t("The setting was not changed.").to_string()),
    }
}
