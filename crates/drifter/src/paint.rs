use smithay::backend::allocator::Fourcc;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::allocator::dumb::DumbAllocator;
use smithay::backend::allocator::format::FormatSet;
use smithay::backend::allocator::gbm::GbmAllocator;
use smithay::backend::drm::DrmDeviceFd;
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::element::RenderElement;
use smithay::backend::renderer::gles::{
    GlesError, GlesFrame, GlesMapping, GlesRenderer, GlesTarget, GlesTexture,
};
use smithay::backend::renderer::pixman::{
    PixmanError, PixmanFrame, PixmanMapping, PixmanRenderer, PixmanTarget, PixmanTexture,
};
use smithay::backend::renderer::sync::SyncPoint;
use smithay::backend::renderer::{
    Bind, Color32F, ContextId, DebugFlags, ExportMem, Frame, ImportDma, ImportDmaWl, ImportMem,
    ImportMemWl, Offscreen, Renderer, RendererSuper, Texture, TextureFilter, TextureMapping,
};
use smithay::output::OutputModeSource;
use smithay::reexports::pixman::Image;
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::utils::{Buffer as BufferCoord, Physical, Point, Rectangle, Scale, Size, Transform};
use smithay::wayland::compositor::SurfaceData;

#[derive(Debug)]
pub enum Painter {
    Gles(GlesRenderer),
    Pixman(PixmanRenderer),
}

pub enum Scanout {
    Gbm(
        DrmCompositor<
            GbmAllocator<DrmDeviceFd>,
            GbmFramebufferExporter<DrmDeviceFd>,
            (),
            DrmDeviceFd,
        >,
    ),
    Dumb(DrmCompositor<DumbAllocator, DrmDeviceFd, (), DrmDeviceFd>),
    Offscreen(Headless),
}

pub struct Headless {
    canvas: Canvas,
    tracker: OutputDamageTracker,
    drawn: bool,
}

macro_rules! each {
    ($scanout:expr, $compositor:ident => $body:expr, $screen:ident => $other:expr) => {
        match $scanout {
            Scanout::Gbm($compositor) => $body,
            Scanout::Dumb($compositor) => $body,
            Scanout::Offscreen($screen) => $other,
        }
    };
}

impl Scanout {
    pub fn offscreen(painter: &mut Painter, source: OutputModeSource) -> Result<Scanout, String> {
        let OutputModeSource::Static { size, .. } = source else {
            return Err("an offscreen head needs a fixed size".to_string());
        };
        let canvas = painter
            .canvas(Fourcc::Abgr8888, (size.w, size.h).into())
            .map_err(|e| format!("no offscreen buffer: {e}"))?;
        Ok(Scanout::Offscreen(Headless {
            canvas,
            tracker: OutputDamageTracker::from_mode_source(source),
            drawn: false,
        }))
    }

    pub fn is_offscreen(&self) -> bool {
        matches!(self, Scanout::Offscreen(_))
    }

    pub fn render_frame<E: RenderElement<Painter>>(
        &mut self,
        renderer: &mut Painter,
        elements: &[E],
        clear: Color32F,
    ) -> Result<bool, String> {
        each!(self, compositor => compositor
        .render_frame(renderer, elements, clear, FrameFlags::DEFAULT)
        .map(|result| result.is_empty)
        .map_err(|e| e.to_string()),
        screen => {
            let age = usize::from(screen.drawn);
            let mut target = renderer.bind(&mut screen.canvas).map_err(|e| e.to_string())?;
            let result = screen
                .tracker
                .render_output(renderer, &mut target, age, elements, clear)
                .map_err(|e| format!("{e:?}"))?;
            let empty = result.damage.is_none_or(|damage| damage.is_empty());
            screen.drawn = true;
            Ok(empty)
        })
    }

    pub fn queue_frame(&mut self) -> Result<(), String> {
        each!(self, compositor => compositor.queue_frame(()).map_err(|e| e.to_string()),
            _screen => Ok(()))
    }

    pub fn frame_submitted(&mut self) -> Result<(), String> {
        each!(self, compositor => compositor.frame_submitted().map(|_| ()).map_err(|e| e.to_string()),
            _screen => Ok(()))
    }

    pub fn reset_state(&mut self) -> Result<(), String> {
        each!(self, compositor => compositor.reset_state().map_err(|e| e.to_string()),
        screen => {
            screen.drawn = false;
            Ok(())
        })
    }

    pub fn clear(&mut self) -> Result<(), String> {
        each!(self, compositor => compositor.clear().map_err(|e| e.to_string()),
            _screen => Ok(()))
    }

    pub fn reset_buffers(&mut self) {
        each!(self, compositor => compositor.reset_buffers(), screen => screen.drawn = false)
    }

    pub fn use_mode(&mut self, mode: smithay::reexports::drm::control::Mode) -> Result<(), String> {
        each!(self, compositor => compositor.use_mode(mode).map_err(|e| e.to_string()),
            _screen => Err("an offscreen head has one mode".to_string()))
    }

    pub fn set_output_mode_source(&mut self, source: OutputModeSource) {
        each!(self, compositor => compositor.set_output_mode_source(source),
        screen => {
            screen.tracker = OutputDamageTracker::from_mode_source(source);
            screen.drawn = false;
        })
    }
}

#[derive(Debug, Clone)]
pub enum Tex {
    Gles(GlesTexture),
    Pixman(PixmanTexture),
}

#[derive(Debug)]
pub enum Target<'a> {
    Gles(GlesTarget<'a>),
    Pixman(PixmanTarget<'a>),
}

pub enum Canvas {
    Gles(GlesTexture),
    Pixman(Image<'static, 'static>),
}

#[derive(Debug)]
pub enum Mapping {
    Gles(GlesMapping),
    Pixman(PixmanMapping),
}

#[derive(Debug)]
pub enum PaintFrame<'frame, 'buffer> {
    Gles(GlesFrame<'frame, 'buffer>),
    Pixman(PixmanFrame<'frame, 'buffer>),
}

#[derive(Debug)]
pub enum PaintError {
    Gles(GlesError),
    Pixman(PixmanError),
    Mixed,
}

impl std::fmt::Display for PaintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaintError::Gles(error) => error.fmt(f),
            PaintError::Pixman(error) => error.fmt(f),
            PaintError::Mixed => f.write_str("a gles and a pixman object were mixed"),
        }
    }
}

impl std::error::Error for PaintError {}

impl From<GlesError> for PaintError {
    fn from(error: GlesError) -> Self {
        PaintError::Gles(error)
    }
}

impl From<PixmanError> for PaintError {
    fn from(error: PixmanError) -> Self {
        PaintError::Pixman(error)
    }
}

impl Painter {
    pub fn is_pixman(&self) -> bool {
        matches!(self, Painter::Pixman(_))
    }

    pub fn render_formats(&self) -> FormatSet {
        let formats = match self {
            Painter::Gles(renderer) => Bind::<Dmabuf>::supported_formats(renderer),
            Painter::Pixman(renderer) => Bind::<Dmabuf>::supported_formats(renderer),
        };
        formats.unwrap_or_default()
    }

    pub fn canvas(
        &mut self,
        format: Fourcc,
        size: Size<i32, BufferCoord>,
    ) -> Result<Canvas, PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(Canvas::Gles(Offscreen::<GlesTexture>::create_buffer(
                renderer, format, size,
            )?)),
            Painter::Pixman(renderer) => Ok(Canvas::Pixman(
                Offscreen::<Image<'static, 'static>>::create_buffer(renderer, format, size)?,
            )),
        }
    }
}

impl Texture for Tex {
    fn width(&self) -> u32 {
        match self {
            Tex::Gles(texture) => texture.width(),
            Tex::Pixman(texture) => texture.width(),
        }
    }

    fn height(&self) -> u32 {
        match self {
            Tex::Gles(texture) => texture.height(),
            Tex::Pixman(texture) => texture.height(),
        }
    }

    fn size(&self) -> Size<i32, BufferCoord> {
        match self {
            Tex::Gles(texture) => texture.size(),
            Tex::Pixman(texture) => texture.size(),
        }
    }

    fn format(&self) -> Option<Fourcc> {
        match self {
            Tex::Gles(texture) => texture.format(),
            Tex::Pixman(texture) => texture.format(),
        }
    }
}

impl Texture for Target<'_> {
    fn width(&self) -> u32 {
        match self {
            Target::Gles(target) => target.width(),
            Target::Pixman(target) => target.width(),
        }
    }

    fn height(&self) -> u32 {
        match self {
            Target::Gles(target) => target.height(),
            Target::Pixman(target) => target.height(),
        }
    }

    fn size(&self) -> Size<i32, BufferCoord> {
        match self {
            Target::Gles(target) => target.size(),
            Target::Pixman(target) => target.size(),
        }
    }

    fn format(&self) -> Option<Fourcc> {
        match self {
            Target::Gles(target) => target.format(),
            Target::Pixman(target) => target.format(),
        }
    }
}

impl Texture for Mapping {
    fn width(&self) -> u32 {
        match self {
            Mapping::Gles(mapping) => mapping.width(),
            Mapping::Pixman(mapping) => mapping.width(),
        }
    }

    fn height(&self) -> u32 {
        match self {
            Mapping::Gles(mapping) => mapping.height(),
            Mapping::Pixman(mapping) => mapping.height(),
        }
    }

    fn size(&self) -> Size<i32, BufferCoord> {
        match self {
            Mapping::Gles(mapping) => mapping.size(),
            Mapping::Pixman(mapping) => mapping.size(),
        }
    }

    fn format(&self) -> Option<Fourcc> {
        match self {
            Mapping::Gles(mapping) => Texture::format(mapping),
            Mapping::Pixman(mapping) => Texture::format(mapping),
        }
    }
}

impl TextureMapping for Mapping {
    fn flipped(&self) -> bool {
        match self {
            Mapping::Gles(mapping) => mapping.flipped(),
            Mapping::Pixman(mapping) => mapping.flipped(),
        }
    }
}

impl Frame for PaintFrame<'_, '_> {
    type Error = PaintError;
    type TextureId = Tex;

    fn context_id(&self) -> ContextId<Tex> {
        match self {
            PaintFrame::Gles(frame) => Frame::context_id(frame).map(),
            PaintFrame::Pixman(frame) => Frame::context_id(frame).map(),
        }
    }

    fn clear(
        &mut self,
        color: Color32F,
        at: &[Rectangle<i32, Physical>],
    ) -> Result<(), PaintError> {
        match self {
            PaintFrame::Gles(frame) => Ok(Frame::clear(frame, color, at)?),
            PaintFrame::Pixman(frame) => Ok(Frame::clear(frame, color, at)?),
        }
    }

    fn draw_solid(
        &mut self,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        color: Color32F,
    ) -> Result<(), PaintError> {
        match self {
            PaintFrame::Gles(frame) => Ok(Frame::draw_solid(frame, dst, damage, color)?),
            PaintFrame::Pixman(frame) => Ok(Frame::draw_solid(frame, dst, damage, color)?),
        }
    }

    fn render_texture_at(
        &mut self,
        texture: &Tex,
        pos: Point<i32, Physical>,
        texture_scale: i32,
        output_scale: impl Into<Scale<f64>>,
        src_transform: Transform,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        alpha: f32,
    ) -> Result<(), PaintError> {
        match (self, texture) {
            (PaintFrame::Gles(frame), Tex::Gles(texture)) => Ok(Frame::render_texture_at(
                frame,
                texture,
                pos,
                texture_scale,
                output_scale,
                src_transform,
                damage,
                opaque_regions,
                alpha,
            )?),
            (PaintFrame::Pixman(frame), Tex::Pixman(texture)) => Ok(Frame::render_texture_at(
                frame,
                texture,
                pos,
                texture_scale,
                output_scale,
                src_transform,
                damage,
                opaque_regions,
                alpha,
            )?),
            _ => Err(PaintError::Mixed),
        }
    }

    fn render_texture_from_to(
        &mut self,
        texture: &Tex,
        src: Rectangle<f64, BufferCoord>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
        opaque_regions: &[Rectangle<i32, Physical>],
        src_transform: Transform,
        alpha: f32,
    ) -> Result<(), PaintError> {
        match (self, texture) {
            (PaintFrame::Gles(frame), Tex::Gles(texture)) => Ok(Frame::render_texture_from_to(
                frame,
                texture,
                src,
                dst,
                damage,
                opaque_regions,
                src_transform,
                alpha,
            )?),
            (PaintFrame::Pixman(frame), Tex::Pixman(texture)) => Ok(Frame::render_texture_from_to(
                frame,
                texture,
                src,
                dst,
                damage,
                opaque_regions,
                src_transform,
                alpha,
            )?),
            _ => Err(PaintError::Mixed),
        }
    }

    fn transformation(&self) -> Transform {
        match self {
            PaintFrame::Gles(frame) => Frame::transformation(frame),
            PaintFrame::Pixman(frame) => Frame::transformation(frame),
        }
    }

    fn wait(&mut self, sync: &SyncPoint) -> Result<(), PaintError> {
        match self {
            PaintFrame::Gles(frame) => Ok(Frame::wait(frame, sync)?),
            PaintFrame::Pixman(frame) => Ok(Frame::wait(frame, sync)?),
        }
    }

    fn finish(self) -> Result<SyncPoint, PaintError> {
        match self {
            PaintFrame::Gles(frame) => Ok(Frame::finish(frame)?),
            PaintFrame::Pixman(frame) => Ok(Frame::finish(frame)?),
        }
    }
}

impl RendererSuper for Painter {
    type Error = PaintError;
    type TextureId = Tex;
    type Framebuffer<'buffer> = Target<'buffer>;
    type Frame<'frame, 'buffer>
        = PaintFrame<'frame, 'buffer>
    where
        'buffer: 'frame;
}

impl Renderer for Painter {
    fn context_id(&self) -> ContextId<Tex> {
        match self {
            Painter::Gles(renderer) => renderer.context_id().map(),
            Painter::Pixman(renderer) => renderer.context_id().map(),
        }
    }

    fn downscale_filter(&mut self, filter: TextureFilter) -> Result<(), PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(renderer.downscale_filter(filter)?),
            Painter::Pixman(renderer) => Ok(renderer.downscale_filter(filter)?),
        }
    }

    fn upscale_filter(&mut self, filter: TextureFilter) -> Result<(), PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(renderer.upscale_filter(filter)?),
            Painter::Pixman(renderer) => Ok(renderer.upscale_filter(filter)?),
        }
    }

    fn set_debug_flags(&mut self, flags: DebugFlags) {
        match self {
            Painter::Gles(renderer) => renderer.set_debug_flags(flags),
            Painter::Pixman(renderer) => renderer.set_debug_flags(flags),
        }
    }

    fn debug_flags(&self) -> DebugFlags {
        match self {
            Painter::Gles(renderer) => renderer.debug_flags(),
            Painter::Pixman(renderer) => renderer.debug_flags(),
        }
    }

    fn render<'frame, 'buffer>(
        &'frame mut self,
        framebuffer: &'frame mut Target<'buffer>,
        output_size: Size<i32, Physical>,
        dst_transform: Transform,
    ) -> Result<PaintFrame<'frame, 'buffer>, PaintError>
    where
        'buffer: 'frame,
    {
        match (self, framebuffer) {
            (Painter::Gles(renderer), Target::Gles(target)) => Ok(PaintFrame::Gles(
                renderer.render(target, output_size, dst_transform)?,
            )),
            (Painter::Pixman(renderer), Target::Pixman(target)) => Ok(PaintFrame::Pixman(
                renderer.render(target, output_size, dst_transform)?,
            )),
            _ => Err(PaintError::Mixed),
        }
    }

    fn wait(&mut self, sync: &SyncPoint) -> Result<(), PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(Renderer::wait(renderer, sync)?),
            Painter::Pixman(renderer) => Ok(Renderer::wait(renderer, sync)?),
        }
    }

    fn cleanup_texture_cache(&mut self) -> Result<(), PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(renderer.cleanup_texture_cache()?),
            Painter::Pixman(renderer) => Ok(renderer.cleanup_texture_cache()?),
        }
    }
}

impl ImportMem for Painter {
    fn import_memory(
        &mut self,
        data: &[u8],
        format: Fourcc,
        size: Size<i32, BufferCoord>,
        flipped: bool,
    ) -> Result<Tex, PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(Tex::Gles(
                renderer.import_memory(data, format, size, flipped)?,
            )),
            Painter::Pixman(renderer) => Ok(Tex::Pixman(
                renderer.import_memory(data, format, size, flipped)?,
            )),
        }
    }

    fn update_memory(
        &mut self,
        texture: &Tex,
        data: &[u8],
        region: Rectangle<i32, BufferCoord>,
    ) -> Result<(), PaintError> {
        match (self, texture) {
            (Painter::Gles(renderer), Tex::Gles(texture)) => {
                Ok(renderer.update_memory(texture, data, region)?)
            }
            (Painter::Pixman(renderer), Tex::Pixman(texture)) => {
                Ok(renderer.update_memory(texture, data, region)?)
            }
            _ => Err(PaintError::Mixed),
        }
    }

    fn mem_formats(&self) -> Box<dyn Iterator<Item = Fourcc>> {
        match self {
            Painter::Gles(renderer) => renderer.mem_formats(),
            Painter::Pixman(renderer) => renderer.mem_formats(),
        }
    }
}

impl ImportMemWl for Painter {
    fn import_shm_buffer(
        &mut self,
        buffer: &WlBuffer,
        surface: Option<&SurfaceData>,
        damage: &[Rectangle<i32, BufferCoord>],
    ) -> Result<Tex, PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(Tex::Gles(
                renderer.import_shm_buffer(buffer, surface, damage)?,
            )),
            Painter::Pixman(renderer) => Ok(Tex::Pixman(
                renderer.import_shm_buffer(buffer, surface, damage)?,
            )),
        }
    }
}

impl ImportDma for Painter {
    fn dmabuf_formats(&self) -> FormatSet {
        match self {
            Painter::Gles(renderer) => renderer.dmabuf_formats(),
            Painter::Pixman(renderer) => renderer.dmabuf_formats(),
        }
    }

    fn import_dmabuf(
        &mut self,
        dmabuf: &Dmabuf,
        damage: Option<&[Rectangle<i32, BufferCoord>]>,
    ) -> Result<Tex, PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(Tex::Gles(renderer.import_dmabuf(dmabuf, damage)?)),
            Painter::Pixman(renderer) => Ok(Tex::Pixman(renderer.import_dmabuf(dmabuf, damage)?)),
        }
    }
}

impl ImportDmaWl for Painter {}

impl Bind<Dmabuf> for Painter {
    fn bind<'a>(&mut self, target: &'a mut Dmabuf) -> Result<Target<'a>, PaintError> {
        match self {
            Painter::Gles(renderer) => Ok(Target::Gles(renderer.bind(target)?)),
            Painter::Pixman(renderer) => Ok(Target::Pixman(renderer.bind(target)?)),
        }
    }

    fn supported_formats(&self) -> Option<FormatSet> {
        Some(self.render_formats())
    }
}

impl Bind<Canvas> for Painter {
    fn bind<'a>(&mut self, target: &'a mut Canvas) -> Result<Target<'a>, PaintError> {
        match (self, target) {
            (Painter::Gles(renderer), Canvas::Gles(texture)) => {
                Ok(Target::Gles(renderer.bind(texture)?))
            }
            (Painter::Pixman(renderer), Canvas::Pixman(image)) => {
                Ok(Target::Pixman(renderer.bind(image)?))
            }
            _ => Err(PaintError::Mixed),
        }
    }
}

impl ExportMem for Painter {
    type TextureMapping = Mapping;

    fn copy_framebuffer(
        &mut self,
        target: &Target<'_>,
        region: Rectangle<i32, BufferCoord>,
        format: Fourcc,
    ) -> Result<Mapping, PaintError> {
        match (self, target) {
            (Painter::Gles(renderer), Target::Gles(target)) => Ok(Mapping::Gles(
                renderer.copy_framebuffer(target, region, format)?,
            )),
            (Painter::Pixman(renderer), Target::Pixman(target)) => Ok(Mapping::Pixman(
                renderer.copy_framebuffer(target, region, format)?,
            )),
            _ => Err(PaintError::Mixed),
        }
    }

    fn copy_texture(
        &mut self,
        texture: &Tex,
        region: Rectangle<i32, BufferCoord>,
        format: Fourcc,
    ) -> Result<Mapping, PaintError> {
        match (self, texture) {
            (Painter::Gles(renderer), Tex::Gles(texture)) => Ok(Mapping::Gles(
                renderer.copy_texture(texture, region, format)?,
            )),
            (Painter::Pixman(renderer), Tex::Pixman(texture)) => Ok(Mapping::Pixman(
                renderer.copy_texture(texture, region, format)?,
            )),
            _ => Err(PaintError::Mixed),
        }
    }

    fn can_read_texture(&mut self, texture: &Tex) -> Result<bool, PaintError> {
        match (self, texture) {
            (Painter::Gles(renderer), Tex::Gles(texture)) => {
                Ok(renderer.can_read_texture(texture)?)
            }
            (Painter::Pixman(renderer), Tex::Pixman(texture)) => {
                Ok(renderer.can_read_texture(texture)?)
            }
            _ => Err(PaintError::Mixed),
        }
    }

    fn map_texture<'a>(&mut self, mapping: &'a Mapping) -> Result<&'a [u8], PaintError> {
        match (self, mapping) {
            (Painter::Gles(renderer), Mapping::Gles(mapping)) => Ok(renderer.map_texture(mapping)?),
            (Painter::Pixman(renderer), Mapping::Pixman(mapping)) => {
                Ok(renderer.map_texture(mapping)?)
            }
            _ => Err(PaintError::Mixed),
        }
    }
}
