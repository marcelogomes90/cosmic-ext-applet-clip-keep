use std::collections::HashMap;

use cosmic_protocols::toplevel_info::v1::client::{
    zcosmic_toplevel_handle_v1::{self, State as ToplevelState, ZcosmicToplevelHandleV1},
    zcosmic_toplevel_info_v1::{self, ZcosmicToplevelInfoV1},
};
use wayland_client::backend::ObjectId;
use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};

use crate::clip::runtime::Runtime;

#[derive(Debug)]
pub struct CosmicData {
    pub foreign: Option<ObjectId>,
}

#[derive(Debug, Default)]
struct Window {
    handle: Option<ExtForeignToplevelHandleV1>,
    cosmic: Option<ZcosmicToplevelHandleV1>,
    app_id: Option<String>,
    activated: bool,
}

pub struct Toplevels {
    list: ExtForeignToplevelListV1,
    info: Option<ZcosmicToplevelInfoV1>,
    windows: HashMap<ObjectId, Window>,
    focused: Option<ObjectId>,
}

impl Toplevels {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Runtime>, _seat: &WlSeat) -> Option<Self> {
        let Ok(list) = globals.bind::<ExtForeignToplevelListV1, _, _>(qh, 1..=1, ()) else {
            tracing::info!("no toplevel list; copies will not be attributed to an application");
            return None;
        };

        let info = globals
            .bind::<ZcosmicToplevelInfoV1, _, _>(qh, 2..=3, ())
            .ok();
        if info.is_none() {
            tracing::info!("no toplevel info; focus cannot be tracked");
        }

        Some(Self {
            list,
            info,
            windows: HashMap::new(),
            focused: None,
        })
    }

    pub fn focused_app(&self) -> Option<String> {
        let id = self.focused.as_ref()?;
        self.windows.get(id)?.app_id.clone()
    }

    fn opened(&mut self, handle: ExtForeignToplevelHandleV1, qh: &QueueHandle<Runtime>) {
        let id = handle.id();
        let cosmic = self.info.as_ref().map(|info| {
            info.get_cosmic_toplevel(
                &handle,
                qh,
                CosmicData {
                    foreign: Some(id.clone()),
                },
            )
        });

        self.windows.insert(
            id,
            Window {
                handle: Some(handle),
                cosmic,
                ..Window::default()
            },
        );
    }

    fn named(&mut self, window: &ObjectId, app_id: String) {
        if let Some(window) = self.windows.get_mut(window) {
            window.app_id = Some(app_id);
        }
    }

    fn closed(&mut self, window: &ObjectId) {
        if let Some(window) = self.windows.remove(window) {
            if let Some(cosmic) = window.cosmic {
                cosmic.destroy();
            }
            if let Some(handle) = window.handle {
                handle.destroy();
            }
        }
    }

    fn restated(&mut self, window: &ObjectId, activated: bool) -> bool {
        let Some(entry) = self.windows.get_mut(window) else {
            return false;
        };

        entry.activated = activated;
        if !activated {
            return false;
        }

        let changed = self.focused.as_ref() != Some(window);
        self.focused = Some(window.clone());
        changed
    }

    fn owner(handle: &ZcosmicToplevelHandleV1) -> Option<ObjectId> {
        handle.data::<CosmicData>()?.foreign.clone()
    }
}

impl Drop for Toplevels {
    fn drop(&mut self) {
        for (_, window) in self.windows.drain() {
            if let Some(cosmic) = window.cosmic {
                cosmic.destroy();
            }
            if let Some(handle) = window.handle {
                handle.destroy();
            }
        }
        self.info = None;
        self.list.destroy();
    }
}

fn states(bytes: &[u8]) -> impl Iterator<Item = u32> + '_ {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .copied()
        .map(u32::from_ne_bytes)
}

fn is_activated(bytes: &[u8]) -> bool {
    states(bytes).any(|state| state == ToplevelState::Activated as u32)
}

impl Dispatch<ExtForeignToplevelListV1, ()> for Runtime {
    event_created_child!(Runtime, ExtForeignToplevelListV1, [
        0 => (ExtForeignToplevelHandleV1, ())
    ]);

    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        (): &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } => {
                state.on_toplevel_opened(toplevel, qh);
            }
            ext_foreign_toplevel_list_v1::Event::Finished => {
                tracing::debug!("the compositor stopped listing windows");
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for Runtime {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                state.on_toplevel_named(&handle.id(), app_id);
            }
            ext_foreign_toplevel_handle_v1::Event::Closed => {
                state.on_toplevel_closed(&handle.id());
            }

            _ => {}
        }
    }
}

impl Dispatch<ZcosmicToplevelInfoV1, ()> for Runtime {
    event_created_child!(Runtime, ZcosmicToplevelInfoV1, [
        0 => (ZcosmicToplevelHandleV1, CosmicData { foreign: None })
    ]);

    fn event(
        _: &mut Self,
        _: &ZcosmicToplevelInfoV1,
        _: zcosmic_toplevel_info_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZcosmicToplevelHandleV1, CosmicData> for Runtime {
    fn event(
        state: &mut Self,
        handle: &ZcosmicToplevelHandleV1,
        event: zcosmic_toplevel_handle_v1::Event,
        _: &CosmicData,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let zcosmic_toplevel_handle_v1::Event::State { state: reported } = event else {
            return;
        };

        if let Some(window) = Toplevels::owner(handle) {
            state.on_toplevel_focus(&window, is_activated(&reported));
        }
    }
}

impl Runtime {
    pub(crate) fn on_toplevel_opened(
        &mut self,
        handle: ExtForeignToplevelHandleV1,
        qh: &QueueHandle<Runtime>,
    ) {
        if let Some(toplevels) = self.toplevels.as_mut() {
            toplevels.opened(handle, qh);
        }
    }

    pub(crate) fn on_toplevel_named(&mut self, window: &ObjectId, app_id: String) {
        if let Some(toplevels) = self.toplevels.as_mut() {
            toplevels.named(window, app_id);
        }
        self.refresh_focused_app();
    }

    pub(crate) fn on_toplevel_closed(&mut self, window: &ObjectId) {
        if let Some(toplevels) = self.toplevels.as_mut() {
            toplevels.closed(window);
        }
    }

    pub(crate) fn on_toplevel_focus(&mut self, window: &ObjectId, activated: bool) {
        let changed = self
            .toplevels
            .as_mut()
            .is_some_and(|toplevels| toplevels.restated(window, activated));

        if changed {
            self.refresh_focused_app();
        }
    }

    fn refresh_focused_app(&mut self) {
        let app = self.toplevels.as_ref().and_then(Toplevels::focused_app);

        if app != self.focused_app {
            tracing::info!(?app, "focus moved");
            self.focused_app = app;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_activated_window_is_recognised_in_a_state_array() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(ToplevelState::Maximized as u32).to_ne_bytes());
        bytes.extend_from_slice(&(ToplevelState::Activated as u32).to_ne_bytes());
        assert!(is_activated(&bytes));
    }

    #[test]
    fn a_window_without_the_activated_state_is_not_focused() {
        let bytes = (ToplevelState::Minimized as u32).to_ne_bytes();
        assert!(!is_activated(&bytes));
    }

    #[test]
    fn an_empty_state_array_is_not_focused() {
        assert!(!is_activated(&[]));
    }

    #[test]
    fn a_truncated_state_array_is_ignored_rather_than_misread() {
        assert!(!is_activated(&[2, 0, 0]));
    }
}
