use super::*;
use smithay::backend::drm::DrmNode;
use smithay::wayland::drm_lease::{
    DrmLease, DrmLeaseBuilder, DrmLeaseHandler, DrmLeaseRequest, DrmLeaseState, LeaseRejected,
};

#[derive(Debug)]
struct Refused(String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

fn refuse(reason: impl Into<String>) -> LeaseRejected {
    let reason = reason.into();
    log!("drifter: a lease was refused: {reason}");
    LeaseRejected::with_cause(Refused(reason))
}

pub(super) fn non_desktop(drm: &DrmDevice, handle: connector::Handle) -> bool {
    let Ok(props) = drm.get_properties(handle) else {
        return false;
    };
    props.iter().any(|(prop, value)| {
        *value == 1
            && drm
                .get_property(*prop)
                .is_ok_and(|info| info.name().to_bytes() == b"non-desktop")
    })
}

impl State {
    pub(super) fn start_leases(&mut self) {
        let Some(drm) = self.drm.as_ref() else {
            return;
        };
        let node = match DrmNode::from_file(drm.device_fd().clone()) {
            Ok(node) => node,
            Err(error) => {
                log!("drifter: headsets cannot be lent to apps: {error}");
                return;
            }
        };
        match DrmLeaseState::new_with_filter::<State, _>(&self.wayland.handle, &node, crate::protocols::trusted) {
            Ok(state) => self.wayland.extras.drm_lease = Some(state),
            Err(error) => log!("drifter: headsets cannot be lent to apps: {error}"),
        }
    }

    pub(super) fn offer_leases(&mut self, headsets: &[(connector::Handle, connector::Info)]) {
        if self.wayland.extras.drm_lease.is_none() {
            return;
        }
        let gone: Vec<connector::Handle> = self
            .lent_out
            .iter()
            .copied()
            .filter(|held| !headsets.iter().any(|(handle, _)| handle == held))
            .collect();
        for handle in gone {
            if let Some(state) = self.wayland.extras.drm_lease.as_mut() {
                state.withdraw_connector(handle);
            }
            self.lent_out.retain(|held| *held != handle);
            self.wayland.extras.leases.retain(|lease| !lease.connectors().any(|each| *each == handle));
        }
        for (handle, info) in headsets {
            if self.lent_out.contains(handle) {
                continue;
            }
            let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
            let description = drifter_core::hdr::model(&name).unwrap_or_else(|| name.clone());
            if let Some(state) = self.wayland.extras.drm_lease.as_mut() {
                state.add_connector::<State>(*handle, name, description);
            }
            self.lent_out.push(*handle);
        }
    }

    pub(super) fn pause_leases(&mut self) {
        if let Some(state) = self.wayland.extras.drm_lease.as_mut() {
            state.suspend();
        }
    }

    pub(super) fn resume_leases(&mut self) {
        if let Some(state) = self.wayland.extras.drm_lease.as_mut() {
            state.resume::<State>();
        }
    }

    pub(super) fn lease_crtcs(&self) -> Vec<crtc::Handle> {
        let mut used: Vec<crtc::Handle> = self.heads.iter().map(|head| head.crtc).collect();
        for lease in &self.wayland.extras.leases {
            used.extend(lease.crtcs().copied());
        }
        used
    }
}

impl DrmLeaseHandler for State {
    fn drm_lease_state(&mut self, _node: DrmNode) -> &mut DrmLeaseState {
        self.wayland
            .extras
            .drm_lease
            .as_mut()
            .expect("a lease request only comes through the lease global")
    }

    fn lease_request(&mut self, _node: DrmNode, request: DrmLeaseRequest) -> Result<DrmLeaseBuilder, LeaseRejected> {
        let mut used = self.lease_crtcs();
        let drm = self.drm.as_ref().ok_or_else(|| refuse("there is no graphics card"))?;
        let resources = drm.resource_handles().map_err(|error| refuse(error.to_string()))?;
        let mut builder = DrmLeaseBuilder::new(drm);
        for handle in request.connectors {
            if !self.lent_out.contains(&handle) {
                return Err(refuse("that connector is not a headset Drifter lends"));
            }
            if self.wayland.extras.leases.iter().any(|lease| lease.connectors().any(|each| *each == handle)) {
                return Err(refuse("that headset is already lent to an app"));
            }
            let info = drm.get_connector(handle, false).map_err(|error| refuse(error.to_string()))?;
            if info.state() != connector::State::Connected {
                return Err(refuse("the headset is unplugged"));
            }
            let crtc = heads::pick_crtc(drm, &resources, &info, &used).ok_or_else(|| refuse("no free display engine"))?;
            let planes = drm.planes(&crtc).map_err(|error| refuse(error.to_string()))?;
            let (plane, claim) = planes
                .primary
                .iter()
                .find_map(|plane| drm.claim_plane(plane.handle, crtc).map(|claim| (plane.handle, claim)))
                .ok_or_else(|| refuse("no free plane for the headset"))?;
            builder.add_connector(handle);
            builder.add_crtc(crtc);
            builder.add_plane(plane, claim);
            used.push(crtc);
        }
        Ok(builder)
    }

    fn new_active_lease(&mut self, _node: DrmNode, lease: DrmLease) {
        out!("lease       lent {} headset(s) to an app", lease.connectors().count());
        self.wayland.extras.leases.push(lease);
    }

    fn lease_destroyed(&mut self, _node: DrmNode, lease_id: u32) {
        self.wayland.extras.leases.retain(|lease| lease.id() != lease_id);
        self.kick_now();
    }
}

smithay::delegate_drm_lease!(State);
