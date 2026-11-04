use super::*;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::Weak;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Monotonic, Time};
use smithay::wayland::commit_timing::{CommitTimerBarrierStateUserData, CommitTimerStateUserData};
use smithay::wayland::compositor::{get_parent, with_states};
use smithay::wayland::fifo::FifoBarrierCachedState;

const UNSEEN_RELEASE: Duration = Duration::from_millis(1000);
const FALLBACK_FRAME: Duration = Duration::from_micros(16_667);
const AFTER_VBLANK: Duration = Duration::from_micros(500);

enum Scope<'a> {
    All,
    Roots(&'a std::collections::HashSet<ObjectId>),
    NotRoots(&'a std::collections::HashSet<ObjectId>),
}

pub(super) fn monotonic_now() -> Duration {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    Duration::new(now.tv_sec.max(0) as u64, now.tv_nsec.clamp(0, 999_999_999) as u32)
}

fn as_duration(time: Time<Monotonic>) -> Duration {
    let spec = smithay::reexports::rustix::time::Timespec::from(time);
    let seconds = spec.tv_sec.max(0) as u64;
    let nanos = spec.tv_nsec.max(0) as u64;
    Duration::from_secs(seconds.saturating_add(nanos / 1_000_000_000)) + Duration::from_nanos(nanos % 1_000_000_000)
}

fn times(span: Duration, steps: u128) -> Duration {
    Duration::from_nanos(span.as_nanos().saturating_mul(steps).min(u128::from(u64::MAX)) as u64)
}

pub(super) fn next_vblank(now: Duration, span: Duration, last: Option<Duration>) -> Duration {
    let span = span.max(Duration::from_micros(100));
    match last {
        Some(last) if last <= now => {
            let steps = (now - last).as_nanos() / span.as_nanos() + 1;
            last + times(span, steps)
        }
        _ => now + span,
    }
}

pub(super) fn wake_for(target: Duration, now: Duration, span: Duration, last: Option<Duration>) -> Duration {
    let span = span.max(Duration::from_micros(100));
    let wake = match last {
        Some(last) if last < target => {
            let steps = (target - last - Duration::from_nanos(1)).as_nanos() / span.as_nanos();
            last + times(span, steps) + AFTER_VBLANK
        }
        _ => target.saturating_sub(span) + AFTER_VBLANK,
    };
    wake.max(now)
}

fn root_of(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = get_parent(&root) {
        root = parent;
    }
    root
}

impl State {
    pub fn track_surface(&mut self, surface: &WlSurface) {
        self.paced.retain(|weak| weak.upgrade().is_ok());
        self.paced.push(surface.downgrade());
        smithay::wayland::compositor::add_pre_commit_hook::<State, _>(surface, |state, _, surface| {
            let timed = with_states(surface, |states| {
                states
                    .data_map
                    .get::<CommitTimerStateUserData>()
                    .is_some_and(|timer| timer.borrow().timestamp.is_some())
            });
            if timed && !state.timers_queued {
                state.timers_queued = true;
                state.handle.insert_idle(|state| {
                    state.timers_queued = false;
                    state.release_timers();
                });
            }
        });
    }

    pub(super) fn lit_heads(&self) -> bool {
        self.heads.iter().any(|head| !head.asleep)
    }

    fn pace_head(&self) -> Option<&Head> {
        self.heads
            .iter()
            .filter(|head| !head.asleep && head.mode.refresh > 0 && self.mirror_source(head).is_none())
            .max_by_key(|head| head.mode.refresh)
    }

    fn pace(&self) -> (Duration, Option<Duration>) {
        match self.pace_head() {
            Some(head) => (Duration::from_secs_f64(1000.0 / f64::from(head.mode.refresh)), head.vblank),
            None => (FALLBACK_FRAME, None),
        }
    }

    pub(super) fn roots_on_head(&self) -> Vec<WlSurface> {
        let view = Rect::new(Vec2::ZERO, self.screen);
        let mut roots = Vec::new();
        let with_popups = |root: &WlSurface, roots: &mut Vec<WlSurface>| {
            roots.push(root.clone());
            for (popup, _) in smithay::desktop::PopupManager::popups_for_surface(root) {
                roots.push(popup.wl_surface().clone());
            }
        };
        for (index, window) in self.windows.iter().enumerate() {
            if window.hidden {
                continue;
            }
            let seen = self.on_screen(self.window_frame(index));
            if seen.origin.x < view.max().x && seen.max().x > 0.0 && seen.origin.y < view.max().y && seen.max().y > 0.0 {
                with_popups(window.toplevel.wl_surface(), &mut roots);
            }
        }
        let layers: Vec<WlSurface> = smithay::desktop::layer_map_for_output(&self.output)
            .layers()
            .map(|layer| layer.wl_surface().clone())
            .collect();
        for layer in layers {
            with_popups(&layer, &mut roots);
        }
        if self.at == self.hand {
            if let smithay::input::pointer::CursorImageStatus::Surface(surface) = &self.cursor_status {
                roots.push(surface.clone());
            }
            if let Some(icon) = &self.drag_icon {
                roots.push(icon.clone());
            }
        }
        if let Some(held) = self.client_lock.as_ref() {
            roots.extend(held.surfaces.iter().map(|(surface, _)| surface.wl_surface().clone()));
        }
        roots
    }

    fn release_fifo(&mut self, scope: Scope<'_>) {
        let mut freed: Vec<smithay::reexports::wayland_server::Client> = Vec::new();
        self.paced.retain(|weak: &Weak<WlSurface>| weak.upgrade().is_ok());
        for weak in &self.paced {
            let Ok(surface) = weak.upgrade() else {
                continue;
            };
            let wanted = match &scope {
                Scope::All => true,
                Scope::Roots(roots) => roots.contains(&root_of(&surface).id()),
                Scope::NotRoots(roots) => !roots.contains(&root_of(&surface).id()),
            };
            if !wanted {
                continue;
            }
            let released = with_states(&surface, |states| {
                let barrier = states.cached_state.get::<FifoBarrierCachedState>().current().barrier.take();
                barrier.map(|barrier| barrier.signal()).is_some()
            });
            if released
                && let Some(client) = surface.client()
                && !freed.iter().any(|each| each.id() == client.id())
            {
                freed.push(client);
            }
        }
        self.clear_blockers(freed);
    }

    fn clear_blockers(&mut self, freed: Vec<smithay::reexports::wayland_server::Client>) {
        for client in freed {
            if !self.unblock.iter().any(|held| held.id() == client.id()) {
                self.unblock.push(client);
            }
        }
        if self.unblock.is_empty() || self.unblock_queued {
            return;
        }
        self.unblock_queued = true;
        self.handle.insert_idle(|state| {
            state.unblock_queued = false;
            let freed = std::mem::take(&mut state.unblock);
            let handle = state.wayland.handle.clone();
            for client in freed {
                if let Some(data) = client.get_data::<crate::shell::Greeting>() {
                    data.compositor_state.blocker_cleared(state, &handle);
                }
            }
        });
    }

    pub(super) fn hold_until_vblank(&mut self, at: usize, roots: Vec<WlSurface>) {
        if roots.is_empty() {
            return;
        }
        self.heads[at].latched.extend(roots);
        if self.latch_timer {
            return;
        }
        let head = &self.heads[at];
        let span = match head.mode.refresh {
            0 => FALLBACK_FRAME,
            hz => Duration::from_secs_f64(1000.0 / f64::from(hz)),
        };
        let now = monotonic_now();
        let wait = next_vblank(now, span, head.vblank).saturating_sub(now).max(AFTER_VBLANK);
        self.latch_timer = self
            .handle
            .insert_source(Timer::from_duration(wait), |_, _, state: &mut State| {
                state.latch_timer = false;
                for index in 0..state.heads.len() {
                    if state.heads[index].awaiting {
                        continue;
                    }
                    let latched = std::mem::take(&mut state.heads[index].latched);
                    state.release_shown(latched);
                }
                TimeoutAction::Drop
            })
            .is_ok();
    }

    pub(super) fn release_shown(&mut self, roots: Vec<WlSurface>) {
        if roots.is_empty() {
            return;
        }
        let ids: std::collections::HashSet<ObjectId> = roots.iter().map(|root| root.id()).collect();
        self.release_fifo(Scope::Roots(&ids));
    }

    pub(super) fn release_unseen_now(&mut self) {
        let ids: std::collections::HashSet<ObjectId> = self
            .heads
            .iter()
            .filter(|head| !head.asleep)
            .flat_map(|head| head.on.iter().cloned())
            .collect();
        self.release_fifo(Scope::NotRoots(&ids));
        self.release_timers();
    }

    pub(super) fn release_timers(&mut self) {
        let now = monotonic_now();
        let fastest = self.pace();
        let heads: Vec<(std::collections::HashSet<ObjectId>, Duration, Option<Duration>)> = self
            .heads
            .iter()
            .filter(|head| !head.asleep && self.mirror_source(head).is_none())
            .map(|head| {
                let span = match head.mode.refresh {
                    0 => FALLBACK_FRAME,
                    hz => Duration::from_secs_f64(1000.0 / f64::from(hz)),
                };
                (head.on.iter().cloned().collect(), span, head.vblank)
            })
            .collect();
        let mut freed: Vec<smithay::reexports::wayland_server::Client> = Vec::new();
        let mut wake: Option<Duration> = None;
        for weak in &self.paced {
            let Ok(surface) = weak.upgrade() else {
                continue;
            };
            let has_timers = with_states(&surface, |states| states.data_map.get::<CommitTimerBarrierStateUserData>().is_some());
            if !has_timers {
                continue;
            }
            let root = root_of(&surface).id();
            let mut paces: Vec<(Duration, Option<Duration>)> = heads
                .iter()
                .filter(|(on, _, _)| on.contains(&root))
                .map(|(_, span, last)| (*span, *last))
                .collect();
            if paces.is_empty() {
                paces.push(fastest);
            }
            let shown_at = paces
                .iter()
                .map(|(span, last)| next_vblank(now, *span, *last))
                .min()
                .unwrap_or(now + FALLBACK_FRAME);
            let (released, next) = with_states(&surface, |states| {
                let Some(timers) = states.data_map.get::<CommitTimerBarrierStateUserData>() else {
                    return (false, None);
                };
                let mut timers = timers.lock().unwrap_or_else(|poison| poison.into_inner());
                let released = timers.signal_until(Time::<Monotonic>::from(shown_at));
                (released, timers.next_deadline().map(|next| as_duration(Time::<Monotonic>::from(next))))
            });
            if released
                && let Some(client) = surface.client()
                && !freed.iter().any(|each| each.id() == client.id())
            {
                freed.push(client);
            }
            if let Some(next) = next {
                let at = paces
                    .iter()
                    .map(|(span, last)| wake_for(next, now, *span, *last))
                    .max()
                    .unwrap_or(next);
                wake = Some(wake.map_or(at, |held| held.min(at)));
            }
        }
        self.clear_blockers(freed);
        self.arm_commit_timer(wake, now);
    }

    fn arm_commit_timer(&mut self, wake: Option<Duration>, now: Duration) {
        if let Some(token) = self.commit_timer.take() {
            self.handle.remove(token);
        }
        let Some(wake) = wake else {
            return;
        };
        let wait = wake.saturating_sub(now).max(AFTER_VBLANK);
        let armed = self.handle.insert_source(Timer::from_duration(wait), |_, _, state: &mut State| {
            state.commit_timer = None;
            state.release_timers();
            state.kick_now();
            TimeoutAction::Drop
        });
        match armed {
            Ok(token) => self.commit_timer = Some(token),
            Err(error) => log!("drifter: cannot time a client's commit: {error}"),
        }
    }

    pub(super) fn release_unseen(&mut self) {
        if self.unseen_timer {
            return;
        }
        self.unseen_timer = self
            .handle
            .insert_source(Timer::from_duration(UNSEEN_RELEASE), |_, _, state: &mut State| {
                state.unseen_timer = false;
                if !state.active || state.dark || !state.lit_heads() {
                    state.release_fifo(Scope::All);
                    state.release_timers();
                    state.release_unseen();
                }
                TimeoutAction::Drop
            })
            .is_ok();
    }
}

#[cfg(test)]
mod tests {
    use super::{next_vblank, wake_for};
    use std::time::Duration;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_next_vblank_follows_the_last_one() {
        assert_eq!(next_vblank(ms(105), ms(10), Some(ms(100))), ms(110));
        assert_eq!(next_vblank(ms(110), ms(10), Some(ms(100))), ms(120));
        assert_eq!(next_vblank(ms(105), ms(10), None), ms(115));
        assert_eq!(next_vblank(ms(95), ms(10), Some(ms(100))), ms(105));
    }

    #[test]
    fn a_timed_commit_wakes_just_after_the_vblank_before_its_time() {
        let span = ms(10);
        let last = Some(ms(100));
        let wake = wake_for(ms(135), ms(101), span, last);
        assert_eq!(wake, ms(130) + Duration::from_micros(500));
        assert!(next_vblank(wake, span, last) >= ms(135));
        let wake = wake_for(ms(130), ms(101), span, last);
        assert_eq!(wake, ms(120) + Duration::from_micros(500));
        assert!(next_vblank(wake, span, last) >= ms(130));
        assert_eq!(wake_for(ms(50), ms(101), span, last), ms(101));
    }
}
