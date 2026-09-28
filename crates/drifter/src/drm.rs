use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use drifter_core::mode::{self, Mode};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd};
use smithay::backend::session::{Session, libseat::LibSeatSession};
use smithay::reexports::drm::buffer::DrmFourcc;
use smithay::reexports::drm::control::{Device as ControlDevice, connector, crtc};
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::DeviceFd;

pub struct Shown {
    pub device: PathBuf,
    pub connector: String,
    pub width: u16,
    pub height: u16,
    pub refresh: u32,
    pub held: Duration,
}

pub fn show_color(
    session: &mut LibSeatSession,
    path: &Path,
    want: (u32, u32),
    color: u32,
    hold: Duration,
) -> Result<Shown, String> {
    let fd = session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;

    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let (device, _notifier) =
        DrmDevice::new(drm_fd, true).map_err(|e| format!("cannot open drm device: {e}"))?;

    let resources = device
        .resource_handles()
        .map_err(|e| format!("cannot read drm resources: {e}"))?;

    let (conn_handle, conn) = resources
        .connectors()
        .iter()
        .filter_map(|handle| {
            device
                .get_connector(*handle, false)
                .ok()
                .map(|c| (*handle, c))
        })
        .find(|(_, c)| c.state() == connector::State::Connected && !c.modes().is_empty())
        .ok_or_else(|| "no connected connector with modes".to_string())?;

    let name = format!("{}-{}", conn.interface().as_str(), conn.interface_id());

    let modes: Vec<Mode> = conn
        .modes()
        .iter()
        .map(|m| {
            let (w, h) = m.size();
            Mode::new(
                u32::from(w),
                u32::from(h),
                m.vrefresh() * 1000,
                m.mode_type()
                    .contains(smithay::reexports::drm::control::ModeTypeFlags::PREFERRED),
            )
        })
        .collect();

    let picked = mode::select(&modes, Some(want))
        .ok_or_else(|| format!("connector {name} offered no usable mode"))?;

    let drm_mode = *conn
        .modes()
        .iter()
        .find(|m| {
            let (w, h) = m.size();
            u32::from(w) == picked.width
                && u32::from(h) == picked.height
                && m.vrefresh() * 1000 == picked.refresh_mhz
        })
        .ok_or_else(|| "selected mode vanished from the connector".to_string())?;

    let crtc = pick_crtc(&device, &resources, &conn)
        .ok_or_else(|| format!("connector {name} has no usable crtc"))?;

    let (width, height) = drm_mode.size();
    let mut buffer = device
        .create_dumb_buffer(
            (u32::from(width), u32::from(height)),
            DrmFourcc::Xrgb8888,
            32,
        )
        .map_err(|e| format!("cannot create dumb buffer: {e}"))?;

    {
        let mut map = device
            .map_dumb_buffer(&mut buffer)
            .map_err(|e| format!("cannot map dumb buffer: {e}"))?;
        for pixel in map.as_mut().as_chunks_mut::<4>().0 {
            *pixel = color.to_ne_bytes();
        }
    }

    let framebuffer = device
        .add_framebuffer(&buffer, 24, 32)
        .map_err(|e| format!("cannot add framebuffer: {e}"))?;

    let started = Instant::now();
    let result = device.set_crtc(
        crtc,
        Some(framebuffer),
        (0, 0),
        &[conn_handle],
        Some(drm_mode),
    );

    if let Err(e) = result {
        let _ = device.destroy_framebuffer(framebuffer);
        let _ = device.destroy_dumb_buffer(buffer);
        return Err(format!(
            "modeset failed: {e}\n\
             drifter needs drm master, which needs a real tty.\n\
             run it from tty1, not over ssh. smithay only warns when master is denied, so this is the first place it shows."
        ));
    }

    while started.elapsed() < hold {
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = device.destroy_framebuffer(framebuffer);
    let _ = device.destroy_dumb_buffer(buffer);

    Ok(Shown {
        device: path.to_path_buf(),
        connector: name,
        width,
        height,
        refresh: drm_mode.vrefresh(),
        held: started.elapsed(),
    })
}

fn pick_crtc(
    device: &DrmDevice,
    resources: &smithay::reexports::drm::control::ResourceHandles,
    conn: &connector::Info,
) -> Option<crtc::Handle> {
    if let Some(encoder) = conn.current_encoder()
        && let Ok(info) = device.get_encoder(encoder)
        && let Some(crtc) = info.crtc()
    {
        return Some(crtc);
    }
    for encoder in conn.encoders() {
        if let Ok(info) = device.get_encoder(*encoder)
            && let Some(crtc) = resources.filter_crtcs(info.possible_crtcs()).first()
        {
            return Some(*crtc);
        }
    }
    None
}
