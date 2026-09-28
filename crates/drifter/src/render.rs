use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use drifter_core::mode::{self, Mode};
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::{DrmDevice, DrmDeviceFd};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::Color32F;
use smithay::backend::renderer::damage::OutputModeSource;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::session::{Session, libseat::LibSeatSession};
use smithay::reexports::drm::buffer::DrmFourcc;
use smithay::reexports::drm::control::{Device as ControlDevice, connector, crtc};
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::{DeviceFd, Scale, Transform};

pub struct Shown {
    pub device: PathBuf,
    pub connector: String,
    pub width: u16,
    pub height: u16,
    pub refresh: u32,
    pub renderer: String,
    pub frames: u32,
    pub held: Duration,
}

type Compositor = DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmDevice<DrmDeviceFd>, (), DrmDeviceFd>;

pub fn show_color(
    session: &mut LibSeatSession,
    path: &Path,
    want: (u32, u32),
    color: [f32; 4],
    hold: Duration,
) -> Result<Shown, String> {
    let fd = session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;

    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let gbm = GbmDevice::new(drm_fd.clone()).map_err(|e| format!("gbm device: {e}"))?;
    let (drm, _notifier) =
        DrmDevice::new(drm_fd, true).map_err(|e| format!("cannot open drm device: {e}"))?;

    let resources = drm
        .resource_handles()
        .map_err(|e| format!("cannot read drm resources: {e}"))?;

    let (conn_handle, conn) = resources
        .connectors()
        .iter()
        .filter_map(|handle| drm.get_connector(*handle, false).ok().map(|c| (*handle, c)))
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

    let crtc = pick_crtc(&drm, &resources, &conn)
        .ok_or_else(|| format!("connector {name} has no usable crtc"))?;

    let surface = drm
        .create_surface(crtc, drm_mode, &[conn_handle])
        .map_err(|e| format!("cannot create drm surface: {e}"))?;

    let egl_display =
        unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|e| format!("egl display: {e}"))?;
    let context = EGLContext::new(&egl_display).map_err(|e| format!("egl context: {e}"))?;
    let renderer_formats = context.dmabuf_render_formats().clone();
    let mut renderer =
        unsafe { GlesRenderer::new(context) }.map_err(|e| format!("gles renderer: {e}"))?;

    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );

    let (width, height) = drm_mode.size();
    let output = OutputModeSource::Static {
        size: (i32::from(width), i32::from(height)).into(),
        scale: Scale::from(1.0),
        transform: Transform::Normal,
    };

    let mut compositor: Compositor = DrmCompositor::new(
        output,
        surface,
        None,
        allocator,
        gbm.clone(),
        [DrmFourcc::Xrgb8888, DrmFourcc::Argb8888],
        renderer_formats,
        (64, 64).into(),
        Some(gbm.clone()),
    )
    .map_err(|e| format!("drm compositor: {e}"))?;

    let gl = describe(&mut renderer);
    let clear = Color32F::from(color);
    let nothing: [SolidColorRenderElement; 0] = [];

    let started = Instant::now();
    let mut frames = 0u32;
    while started.elapsed() < hold {
        compositor
            .render_frame(&mut renderer, &nothing, clear, FrameFlags::DEFAULT)
            .map_err(|e| {
                format!(
                    "render failed: {e}\n\
                     drifter needs drm master, which needs a real tty.\n\
                     run it from tty1, not over ssh. smithay only warns when master is denied."
                )
            })?;
        compositor
            .queue_frame(())
            .map_err(|e| format!("page flip failed: {e}"))?;
        frames += 1;
        std::thread::sleep(Duration::from_millis(16));
    }

    Ok(Shown {
        device: path.to_path_buf(),
        connector: name,
        width,
        height,
        refresh: drm_mode.vrefresh(),
        renderer: gl,
        frames,
        held: started.elapsed(),
    })
}

fn describe(renderer: &mut GlesRenderer) -> String {
    use smithay::backend::renderer::gles::ffi;
    renderer
        .with_context(|gl| unsafe {
            let raw = gl.GetString(ffi::RENDERER);
            if raw.is_null() {
                "unknown".to_string()
            } else {
                std::ffi::CStr::from_ptr(raw as *const std::ffi::c_char)
                    .to_string_lossy()
                    .into_owned()
            }
        })
        .unwrap_or_else(|_| "unknown".to_string())
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
