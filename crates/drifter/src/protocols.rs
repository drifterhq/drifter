use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::desktop::{
    LayerSurface, PopupKind, PopupManager, WindowSurfaceType, layer_map_for_output,
};
use smithay::input::pointer::PointerHandle;
use smithay::output::Output;
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Rectangle};
use smithay::wayland::compositor::with_states;
use smithay::wayland::cursor_shape::CursorShapeManagerState;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use smithay::wayland::fractional_scale::{
    FractionalScaleHandler, FractionalScaleManagerState, with_fractional_scale,
};
use smithay::wayland::idle_inhibit::{IdleInhibitHandler, IdleInhibitManagerState};
use smithay::wayland::input_method::{
    InputMethodHandler, InputMethodManagerState, PopupSurface as ImePopup,
};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::pointer_constraints::{
    PointerConstraintsHandler, PointerConstraintsState, with_pointer_constraint,
};
use smithay::wayland::relative_pointer::RelativePointerManagerState;
use smithay::wayland::selection::primary_selection::{
    PrimarySelectionHandler, PrimarySelectionState,
};
use smithay::wayland::shell::wlr_layer::{
    Layer, LayerSurfaceData, WlrLayerShellHandler, WlrLayerShellState,
};
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
    Mode as KdeMode, OrgKdeKwinServerDecoration,
};
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDefault;
use smithay::reexports::wayland_server::WEnum;
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};
use smithay::wayland::shell::xdg::PopupSurface;
use smithay::wayland::tablet_manager::TabletSeatHandler;
use smithay::wayland::text_input::TextInputManagerState;
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};
use smithay::{
    delegate_cursor_shape, delegate_dmabuf, delegate_kde_decoration, delegate_fractional_scale, delegate_idle_inhibit,
    delegate_input_method_manager, delegate_layer_shell, delegate_pointer_constraints,
    delegate_primary_selection, delegate_relative_pointer, delegate_text_input_manager,
    delegate_viewporter, delegate_xdg_activation,
};

use crate::render::State;

const ACTIVATION_PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

pub struct Extras {
    pub dmabuf: DmabufState,
    pub dmabuf_global: Option<DmabufGlobal>,
    pub primary: PrimarySelectionState,
    pub activation: XdgActivationState,
    pub layers: WlrLayerShellState,
    pub inhibitors: Vec<WlSurface>,
    _outputs: OutputManagerState,
    _viewporter: ViewporterState,
    _fractional: FractionalScaleManagerState,
    _relative: RelativePointerManagerState,
    _constraints: PointerConstraintsState,
    _idle: IdleInhibitManagerState,
    _text: TextInputManagerState,
    _ime: InputMethodManagerState,
    _cursor_shape: CursorShapeManagerState,
    kde_decoration: KdeDecorationState,
}

pub fn start(handle: &DisplayHandle) -> Extras {
    Extras {
        dmabuf: DmabufState::new(),
        dmabuf_global: None,
        primary: PrimarySelectionState::new::<State>(handle),
        activation: XdgActivationState::new::<State>(handle),
        layers: WlrLayerShellState::new::<State>(handle),
        inhibitors: Vec::new(),
        _outputs: OutputManagerState::new_with_xdg_output::<State>(handle),
        _viewporter: ViewporterState::new::<State>(handle),
        _fractional: FractionalScaleManagerState::new::<State>(handle),
        _relative: RelativePointerManagerState::new::<State>(handle),
        _constraints: PointerConstraintsState::new::<State>(handle),
        _idle: IdleInhibitManagerState::new::<State>(handle),
        _text: TextInputManagerState::new::<State>(handle),
        _ime: InputMethodManagerState::new::<State, _>(handle, |_| true),
        _cursor_shape: CursorShapeManagerState::new::<State>(handle),
        kde_decoration: KdeDecorationState::new::<State>(handle, KdeDefault::Server),
    }
}

impl DmabufHandler for State {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.wayland.extras.dmabuf
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        if self.import_dmabuf(&dmabuf) {
            let _ = notifier.successful::<State>();
        } else {
            notifier.failed();
        }
    }
}

/// GTK 3 ignores xdg-decoration but obeys this older KDE protocol. With
/// Server as the default it draws no title bar of its own, so a GTK 3 app
/// never shows two frames.
impl KdeDecorationHandler for State {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.wayland.extras.kde_decoration
    }

    fn new_decoration(&mut self, surface: &WlSurface, decoration: &OrgKdeKwinServerDecoration) {
        decoration.mode(KdeMode::Server);
        self.set_decorated(surface, true);
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        mode: WEnum<KdeMode>,
    ) {
        if let WEnum::Value(mode) = mode {
            decoration.mode(mode);
            self.set_decorated(surface, mode == KdeMode::Server);
        }
    }
}

impl PrimarySelectionHandler for State {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.wayland.extras.primary
    }
}

impl XdgActivationHandler for State {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.wayland.extras.activation
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if data.timestamp.elapsed() < ACTIVATION_PATIENCE {
            self.activate(&surface);
        }
        self.wayland.extras.activation.remove_token(&token);
    }
}

impl FractionalScaleHandler for State {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        with_states(&surface, |states| {
            with_fractional_scale(states, |scale| scale.set_preferred_scale(1.0));
        });
    }
}

impl PointerConstraintsHandler for State {
    fn new_constraint(&mut self, surface: &WlSurface, pointer: &PointerHandle<Self>) {
        if pointer.current_focus().as_ref() == Some(surface) {
            with_pointer_constraint(surface, pointer, |constraint| {
                if let Some(constraint) = constraint {
                    constraint.activate();
                }
            });
        }
    }

    fn cursor_position_hint(
        &mut self,
        _surface: &WlSurface,
        _pointer: &PointerHandle<Self>,
        _location: Point<f64, Logical>,
    ) {
    }
}

impl IdleInhibitHandler for State {
    fn inhibit(&mut self, surface: WlSurface) {
        let held = &mut self.wayland.extras.inhibitors;
        if !held.contains(&surface) {
            held.push(surface);
        }
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.wayland
            .extras
            .inhibitors
            .retain(|held| held != &surface);
    }
}

impl InputMethodHandler for State {
    fn new_popup(&mut self, surface: ImePopup) {
        if let Err(error) = self.popups.track_popup(PopupKind::from(surface)) {
            eprintln!("drifter: input method popup lost its parent: {error}");
        }
    }

    fn dismiss_popup(&mut self, surface: ImePopup) {
        if let Some(parent) = surface.get_parent().map(|parent| parent.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn popup_repositioned(&mut self, _surface: ImePopup) {}

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.window_geometry(parent).unwrap_or_default()
    }
}

impl TabletSeatHandler for State {}

impl WlrLayerShellHandler for State {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.wayland.extras.layers
    }

    fn new_layer_surface(
        &mut self,
        surface: smithay::wayland::shell::wlr_layer::LayerSurface,
        output: Option<WlOutput>,
        _layer: Layer,
        namespace: String,
    ) {
        let output = output
            .as_ref()
            .and_then(Output::from_resource)
            .unwrap_or_else(|| self.output.clone());
        let mut map = layer_map_for_output(&output);
        if let Err(error) = map.map_layer(&LayerSurface::new(surface, namespace)) {
            eprintln!("drifter: cannot place a layer surface: {error}");
        }
    }

    fn new_popup(
        &mut self,
        _parent: smithay::wayland::shell::wlr_layer::LayerSurface,
        popup: PopupSurface,
    ) {
        if let Err(error) = self.popups.track_popup(PopupKind::Xdg(popup)) {
            eprintln!("drifter: layer popup lost its parent: {error}");
        }
    }

    fn layer_destroyed(&mut self, surface: smithay::wayland::shell::wlr_layer::LayerSurface) {
        let mut map = layer_map_for_output(&self.output);
        let gone = map
            .layers()
            .find(|layer| layer.layer_surface() == &surface)
            .cloned();
        if let Some(layer) = gone {
            map.unmap_layer(&layer);
        }
        drop(map);
        self.layer_gone(surface.wl_surface());
    }
}

pub fn settle_layer(output: &Output, surface: &WlSurface) -> Option<LayerSurface> {
    let mut map = layer_map_for_output(output);
    let layer = map
        .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
        .cloned()?;
    map.arrange();
    let sent = with_states(surface, |states| {
        states
            .data_map
            .get::<LayerSurfaceData>()
            .map(|data| data.lock().unwrap().initial_configure_sent)
            .unwrap_or(true)
    });
    if !sent {
        layer.layer_surface().send_configure();
    }
    Some(layer)
}

delegate_dmabuf!(State);
delegate_primary_selection!(State);
delegate_xdg_activation!(State);
delegate_viewporter!(State);
delegate_fractional_scale!(State);
delegate_relative_pointer!(State);
delegate_pointer_constraints!(State);
delegate_idle_inhibit!(State);
delegate_text_input_manager!(State);
delegate_input_method_manager!(State);
delegate_cursor_shape!(State);
delegate_layer_shell!(State);
delegate_kde_decoration!(State);
