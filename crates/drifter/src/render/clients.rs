use super::*;
use smithay::desktop::utils::{OutputPresentationFeedback, take_presentation_feedback_surface_tree};
use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback::Kind;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::{BufferAssignment, CompositorHandler, SurfaceAttributes, with_states};

impl State {
    pub fn wait_for_buffers(&mut self, surface: &WlSurface) {
        smithay::wayland::compositor::add_pre_commit_hook::<State, _>(surface, |state, _, surface| {
            state.block_until_ready(surface);
        });
    }

    fn block_until_ready(&mut self, surface: &WlSurface) {
        use smithay::wayland::drm_syncobj::DrmSyncobjCachedState;

        let (acquire, dmabuf) = with_states(surface, |states| {
            let acquire = states
                .cached_state
                .get::<DrmSyncobjCachedState>()
                .pending()
                .acquire_point
                .clone();
            let dmabuf = match states.cached_state.get::<SurfaceAttributes>().pending().buffer.as_ref() {
                Some(BufferAssignment::NewBuffer(buffer)) => {
                    smithay::wayland::dmabuf::get_dmabuf(&buffer).ok().cloned()
                }
                _ => None,
            };
            (acquire, dmabuf)
        });
        let Some(dmabuf) = dmabuf else {
            return;
        };
        let Some(client) = surface.client() else {
            return;
        };
        if let Some(acquire) = acquire
            && let Ok((blocker, source)) = acquire.generate_blocker()
        {
            let held = client.clone();
            let watched = self.handle.insert_source(source, move |_, _, state: &mut State| {
                state.unblock(&held);
                Ok(())
            });
            if watched.is_ok() {
                smithay::wayland::compositor::add_blocker(surface, blocker);
                return;
            }
        }
        if let Ok((blocker, source)) =
            dmabuf.generate_blocker(smithay::reexports::calloop::Interest::READ)
        {
            let watched = self.handle.insert_source(source, move |_, _, state: &mut State| {
                state.unblock(&client);
                Ok(())
            });
            if watched.is_ok() {
                smithay::wayland::compositor::add_blocker(surface, blocker);
            }
        }
    }

    fn unblock(&mut self, client: &smithay::reexports::wayland_server::Client) {
        let handle = self.wayland.handle.clone();
        self.client_compositor_state(client).blocker_cleared(self, &handle);
    }

    pub(super) fn client_roots(&self) -> Vec<WlSurface> {
        let mut roots = Vec::new();
        let with_popups = |root: &WlSurface, roots: &mut Vec<WlSurface>| {
            roots.push(root.clone());
            for (popup, _) in smithay::desktop::PopupManager::popups_for_surface(root) {
                roots.push(popup.wl_surface().clone());
            }
        };
        for window in &self.windows {
            with_popups(window.toplevel.wl_surface(), &mut roots);
        }
        for head in &self.heads {
            let layers: Vec<WlSurface> = smithay::desktop::layer_map_for_output(&head.output)
                .layers()
                .map(|layer| layer.wl_surface().clone())
                .collect();
            for layer in layers {
                with_popups(&layer, &mut roots);
            }
        }
        if let smithay::input::pointer::CursorImageStatus::Surface(surface) = &self.cursor_status {
            roots.push(surface.clone());
        }
        if let Some(icon) = &self.drag_icon {
            roots.push(icon.clone());
        }
        if let Some(held) = self.client_lock.as_ref() {
            roots.extend(held.surfaces.iter().map(|(surface, _)| surface.wl_surface().clone()));
        }
        roots
    }

    pub(super) fn gather_feedback(&mut self, at: usize) {
        let output = self.heads[at].output.clone();
        let mut feedback = OutputPresentationFeedback::new(&output);
        for root in self.client_roots() {
            take_presentation_feedback_surface_tree(
                &root,
                &mut feedback,
                |_, _| Some(output.clone()),
                |_, _| Kind::empty(),
            );
        }
        if let Some(mut stale) = self.heads[at].feedback.replace(feedback) {
            stale.discarded();
        }
    }

    pub(super) fn presented(&mut self, at: usize, seen: Option<(Duration, u64)>) {
        let head = &mut self.heads[at];
        let Some(mut feedback) = head.feedback.take() else {
            return;
        };
        let refresh = match head.mode.refresh {
            hz if hz > 0 => smithay::wayland::presentation::Refresh::fixed(Duration::from_secs_f64(
                1000.0 / f64::from(hz),
            )),
            _ => smithay::wayland::presentation::Refresh::Unknown,
        };
        let clock = smithay::utils::Clock::<smithay::utils::Monotonic>::new();
        match seen {
            Some((time, sequence)) => feedback.presented::<_, smithay::utils::Monotonic>(
                time,
                refresh,
                sequence,
                Kind::Vsync | Kind::HwClock | Kind::HwCompletion,
            ),
            None => feedback.presented(clock.now(), refresh, 0, Kind::empty()),
        }
    }

    pub(super) fn wants_tearing(&self) -> bool {
        self.held_by_fullscreen()
            .is_some_and(|index| crate::tearing::wants_tearing(self.windows[index].toplevel.wl_surface()))
    }

    pub fn set_modal(&mut self, toplevel: &smithay::wayland::shell::xdg::ToplevelSurface, modal: bool) {
        let Some(index) = self.windows.iter().position(|window| &window.toplevel == toplevel) else {
            return;
        };
        self.windows[index].modal = modal;
        if modal && self.windows[index].placed {
            self.center_on_parent(index);
            self.focus_window(index);
        }
        self.kick_now();
    }

    pub(super) fn center_on_parent(&mut self, index: usize) -> bool {
        let Some(parent) = self.windows[index].toplevel.parent() else {
            return false;
        };
        let Some(over) = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == &parent && window.placed)
            .map(|window| window.content)
        else {
            return false;
        };
        let size = self.windows[index].content.size;
        let top = over.origin.y + self.chrome.title;
        let origin = Vec2::new(over.center().x - size.x / 2.0, (over.center().y - size.y / 2.0).max(top));
        self.windows[index].content.origin = origin.round();
        true
    }

    pub(super) fn modal_of(&self, index: usize) -> Option<usize> {
        let parent = self.windows[index].toplevel.wl_surface();
        self.windows.iter().position(|window| {
            window.modal && !window.hidden && window.toplevel.parent().as_ref() == Some(parent)
        })
    }

    pub fn listen_sandboxed(
        &mut self,
        source: smithay::wayland::security_context::SecurityContextListenerSource,
        context: smithay::wayland::security_context::SecurityContext,
    ) {
        let watched = self.handle.insert_source(source, move |stream, _, state: &mut State| {
            let data = crate::shell::sandboxed_data(context.clone());
            if let Err(error) = state.wayland.handle.insert_client(stream, data) {
                log!("drifter: cannot greet a sandboxed client: {error}");
            }
        });
        if let Err(error) = watched {
            log!("drifter: cannot listen for sandboxed clients: {error}");
        }
    }

    pub(super) fn draw_mirrors(&mut self, at: usize, clear: smithay::backend::renderer::Color32F) {
        use smithay::backend::renderer::element::utils::{Relocate, RelocateRenderElement};

        let mirrors: Vec<usize> = (0..self.heads.len())
            .filter(|index| *index != at && self.mirror_source(&self.heads[*index]) == Some(at))
            .filter(|index| !self.heads[*index].awaiting)
            .collect();
        if mirrors.is_empty() {
            return;
        }
        let screen = self.screen;
        for index in mirrors {
            let head = &self.heads[index];
            let turned = match head.rotate % 180 {
                90 => Vec2::new(head.pixels.y, head.pixels.x),
                _ => head.pixels,
            };
            let fit = (turned.x / screen.x).min(turned.y / screen.y);
            let offset = ((turned - screen * fit) / 2.0).round();
            let mut pieces = self.elements();
            self.style_fade_elements(&mut pieces);
            let shown: Vec<_> = pieces
                .into_iter()
                .map(|piece| {
                    RelocateRenderElement::from_element(
                        RescaleRenderElement::from_element(piece, (0, 0).into(), fit),
                        (offset.x as i32, offset.y as i32),
                        Relocate::Relative,
                    )
                })
                .collect();
            let State { heads, renderer, .. } = self;
            let head = &mut heads[index];
            match head.compositor.render_frame(renderer, &shown, clear, false) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    log!("drifter: mirror {} failed: {error}", head.name);
                    continue;
                }
            }
            if let Err(error) = head.compositor.queue_frame() {
                log!("drifter: mirror {} cannot flip: {error}", head.name);
                continue;
            }
            head.frames += 1;
            head.awaiting = true;
            self.gather_feedback(index);
        }
    }
}
