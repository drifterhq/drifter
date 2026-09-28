use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use drifter_core::image::Image;
use drifter_core::mode::{self, Mode};
use drifter_core::startup::{Phase, Startup};
use drifter_core::{Rect, Vec2};
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::Color32F;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::session::{Session, libseat::LibSeatSession};
use smithay::output::OutputModeSource;
use smithay::reexports::drm::buffer::DrmFourcc;
use smithay::reexports::drm::control::{Device as ControlDevice, connector, crtc};
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::{DeviceFd, Scale, Transform};

const HOLD_AFTER: Duration = Duration::from_secs(2);

pub struct Shown {
    pub device: PathBuf,
    pub connector: String,
    pub width: u16,
    pub height: u16,
    pub refresh: u32,
    pub renderer: String,
    pub frames: u32,
    pub held: Duration,
    pub splash: Option<Rect>,
    pub wallpaper: Option<Rect>,
    pub phase: Phase,
}

type Compositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

pub fn show(
    session: &mut LibSeatSession,
    path: &Path,
    want: (u32, u32),
    background: [f32; 4],
    splash: Option<&Image>,
    wallpaper: Option<&Image>,
    loading: Duration,
) -> Result<Shown, String> {
    let fd = session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;

    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let gbm = GbmDevice::new(drm_fd.clone()).map_err(|e| format!("gbm device: {e}"))?;
    let (mut drm, _notifier) =
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
        GbmFramebufferExporter::new(gbm.clone(), None),
        [DrmFourcc::Xrgb8888, DrmFourcc::Argb8888],
        renderer_formats,
        (64, 64).into(),
        Some(gbm.clone()),
    )
    .map_err(|e| format!("drm compositor: {e}"))?;

    let gl = describe(&mut renderer);
    let clear = Color32F::from(background);
    let screen = Vec2::new(f64::from(width), f64::from(height));
    let startup = Startup::default();

    let splash_texture = upload(&mut renderer, splash, "splash")?;
    let wallpaper_texture = upload(&mut renderer, wallpaper, "wallpaper")?;

    let splash_rect = splash_texture
        .as_ref()
        .map(|_| Startup::splash_rect(screen));
    let wallpaper_rect = wallpaper
        .zip(wallpaper_texture.as_ref())
        .map(|(image, _)| Startup::wallpaper_rect(image.size(), screen));

    let started = Instant::now();
    let mut frames = 0u32;
    let mut phase = Phase::Loading;

    loop {
        let since_loaded = started.elapsed().checked_sub(loading);
        phase = startup.phase(since_loaded);
        let (splash_alpha, wallpaper_alpha) = startup.alphas(since_loaded);

        let mut elements: Vec<TextureRenderElement<GlesTexture>> = Vec::new();
        if let (Some(buffer), Some(rect)) = (splash_texture.as_ref(), splash_rect)
            && splash_alpha > 0.0
        {
            elements.push(element(buffer, rect, splash_alpha));
        }
        if let (Some(buffer), Some(rect)) = (wallpaper_texture.as_ref(), wallpaper_rect)
            && wallpaper_alpha > 0.0
        {
            elements.push(element(buffer, rect, wallpaper_alpha));
        }

        let result = compositor
            .render_frame(&mut renderer, &elements, clear, FrameFlags::DEFAULT)
            .map_err(|e| {
                if frames == 0 {
                    format!(
                        "first render failed: {e}\n\
                         drifter needs drm master, which needs a real tty.\n\
                         run it from tty1, not over ssh. smithay only warns when master is denied."
                    )
                } else {
                    format!("render failed on frame {frames}: {e}")
                }
            })?;
        if !result.is_empty {
            compositor
                .queue_frame(())
                .map_err(|e| format!("page flip failed on frame {frames}: {e}"))?;
            frames += 1;
        }

        if phase == Phase::Ready && started.elapsed() >= loading + startup.fade + HOLD_AFTER {
            break;
        }
        std::thread::sleep(Duration::from_millis(8));
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
        splash: splash_rect,
        wallpaper: wallpaper_rect,
        phase,
    })
}

fn upload(
    renderer: &mut GlesRenderer,
    image: Option<&Image>,
    what: &str,
) -> Result<Option<TextureBuffer<GlesTexture>>, String> {
    match image {
        None => Ok(None),
        Some(image) => TextureBuffer::from_memory(
            renderer,
            &image.rgba,
            DrmFourcc::Abgr8888,
            (image.width as i32, image.height as i32),
            false,
            1,
            Transform::Normal,
            None,
        )
        .map(Some)
        .map_err(|e| format!("cannot upload {what}: {e}")),
    }
}

fn element(
    buffer: &TextureBuffer<GlesTexture>,
    rect: Rect,
    alpha: f32,
) -> TextureRenderElement<GlesTexture> {
    TextureRenderElement::from_texture_buffer(
        (rect.origin.x, rect.origin.y),
        buffer,
        Some(alpha),
        None,
        Some((rect.size.x as i32, rect.size.y as i32).into()),
        Kind::Unspecified,
    )
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
