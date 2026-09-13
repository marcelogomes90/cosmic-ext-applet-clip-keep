use wayland_client::backend::ObjectId;
use wayland_client::globals::GlobalList;
use wayland_client::protocol::wl_output::{self, WlOutput};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

use crate::clip::runtime::Runtime;

const NAMED: u32 = 4;

struct Screen {
    global: u32,
    output: WlOutput,
    name: Option<String>,
}

#[derive(Default)]
pub struct Outputs {
    screens: Vec<Screen>,
}

impl Outputs {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Runtime>) -> Self {
        let mut outputs = Self::default();

        for global in globals.contents().clone_list() {
            if global.interface == WlOutput::interface().name {
                outputs.add(globals.registry(), global.name, global.version, qh);
            }
        }

        outputs
    }

    pub fn add(
        &mut self,
        registry: &WlRegistry,
        global: u32,
        version: u32,
        qh: &QueueHandle<Runtime>,
    ) {
        if version < NAMED {
            tracing::info!(
                version,
                "the compositor does not name its outputs, so the shortcut cannot pick one"
            );
            return;
        }

        if self.screens.iter().any(|screen| screen.global == global) {
            return;
        }

        let output = registry.bind::<WlOutput, _, _>(global, NAMED, qh, ());
        self.screens.push(Screen {
            global,
            output,
            name: None,
        });
    }

    pub fn remove(&mut self, global: u32) {
        if let Some(position) = self
            .screens
            .iter()
            .position(|screen| screen.global == global)
        {
            self.screens.remove(position).output.release();
        }
    }

    pub fn name(&self, id: &ObjectId) -> Option<&str> {
        self.screens
            .iter()
            .find(|screen| screen.output.id() == *id)
            .and_then(|screen| screen.name.as_deref())
    }

    fn named(&mut self, id: &ObjectId, name: String) {
        if let Some(screen) = self
            .screens
            .iter_mut()
            .find(|screen| screen.output.id() == *id)
        {
            screen.name = Some(name);
        }
    }
}

impl Drop for Outputs {
    fn drop(&mut self) {
        for screen in self.screens.drain(..) {
            screen.output.release();
        }
    }
}

impl Dispatch<WlOutput, ()> for Runtime {
    fn event(
        state: &mut Self,
        output: &WlOutput,
        event: wl_output::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event {
            state.on_output_named(&output.id(), name);
        }
    }
}

impl Runtime {
    pub(crate) fn on_output_named(&mut self, id: &ObjectId, name: String) {
        self.outputs.named(id, name);
        self.refresh_active_output();
    }

    pub(crate) fn on_global_added(
        &mut self,
        registry: &WlRegistry,
        global: u32,
        interface: &str,
        version: u32,
        qh: &QueueHandle<Runtime>,
    ) {
        if interface == WlOutput::interface().name {
            self.outputs.add(registry, global, version, qh);
        }
    }

    pub(crate) fn on_global_removed(&mut self, global: u32) {
        self.outputs.remove(global);
    }

    pub(crate) fn refresh_active_output(&mut self) {
        let Some(name) = self.focused_output() else {
            return;
        };

        if self.active_output.as_deref() != Some(name.as_str()) {
            tracing::debug!(output = %name, "the focused window moved to another output");
            self.active_output = Some(name);
        }
    }

    pub(crate) fn target_output(&self) -> Option<String> {
        self.focused_output().or_else(|| self.active_output.clone())
    }

    fn focused_output(&self) -> Option<String> {
        let id = self.toplevels.as_ref()?.focused_output()?;
        self.outputs.name(id).map(str::to_owned)
    }
}
