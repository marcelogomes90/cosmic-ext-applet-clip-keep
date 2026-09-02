use std::os::fd::BorrowedFd;
use std::sync::{Arc, Mutex};

use wayland_client::globals::{GlobalList, GlobalListContents};
use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols::ext::data_control::v1::client::{
    ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
    ext_data_control_manager_v1::ExtDataControlManagerV1,
    ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
    ext_data_control_source_v1::{self, ExtDataControlSourceV1},
};
use wayland_protocols_wlr::data_control::v1::client::{
    zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
    zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
    zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
    zwlr_data_control_source_v1::{self, ZwlrDataControlSourceV1},
};

use super::SetupError;
use crate::clip::model::Backend;
use crate::clip::runtime::Runtime;

#[derive(Debug, Default)]
pub struct OfferData {
    pub mimes: Mutex<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct SourceData {
    pub flavors: Mutex<Vec<(String, Arc<[u8]>)>>,
}

macro_rules! variant {
    ($self:ident, $binding:ident => $body:expr) => {
        match $self {
            Self::Ext($binding) => $body,
            Self::Wlr($binding) => $body,
        }
    };
}

#[derive(Clone, Debug)]
pub enum Manager {
    Ext(ExtDataControlManagerV1),
    Wlr(ZwlrDataControlManagerV1),
}

impl Manager {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Runtime>) -> Result<Self, SetupError> {
        if let Ok(manager) = globals.bind::<ExtDataControlManagerV1, _, _>(qh, 1..=1, ()) {
            tracing::info!(protocol = Backend::Ext.protocol(), "bound data-control");
            return Ok(Self::Ext(manager));
        }

        if let Ok(manager) = globals.bind::<ZwlrDataControlManagerV1, _, _>(qh, 1..=2, ()) {
            tracing::info!(protocol = Backend::Wlr.protocol(), "bound data-control");
            return Ok(Self::Wlr(manager));
        }

        Err(SetupError::NoDataControl)
    }

    pub fn backend(&self) -> Backend {
        match self {
            Self::Ext(_) => Backend::Ext,
            Self::Wlr(_) => Backend::Wlr,
        }
    }

    pub fn get_data_device(&self, seat: &WlSeat, qh: &QueueHandle<Runtime>) -> Device {
        match self {
            Self::Ext(manager) => Device::Ext(manager.get_data_device(seat, qh, ())),
            Self::Wlr(manager) => Device::Wlr(manager.get_data_device(seat, qh, ())),
        }
    }

    pub fn create_data_source(&self, qh: &QueueHandle<Runtime>) -> Source {
        match self {
            Self::Ext(manager) => {
                Source::Ext(manager.create_data_source(qh, SourceData::default()))
            }
            Self::Wlr(manager) => {
                Source::Wlr(manager.create_data_source(qh, SourceData::default()))
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Device {
    Ext(ExtDataControlDeviceV1),
    Wlr(ZwlrDataControlDeviceV1),
}

impl Device {
    pub fn set_selection(&self, source: Option<&Source>) {
        match (self, source) {
            (Self::Ext(device), Some(Source::Ext(source))) => device.set_selection(Some(source)),
            (Self::Ext(device), None) => device.set_selection(None),
            (Self::Wlr(device), Some(Source::Wlr(source))) => device.set_selection(Some(source)),
            (Self::Wlr(device), None) => device.set_selection(None),
            (device, source) => {
                tracing::error!(?device, ?source, "refusing to mix data-control protocols");
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Offer {
    Ext(ExtDataControlOfferV1),
    Wlr(ZwlrDataControlOfferV1),
}

impl Offer {
    pub fn mimes(&self) -> Vec<String> {
        let data = match self {
            Self::Ext(offer) => offer.data::<OfferData>(),
            Self::Wlr(offer) => offer.data::<OfferData>(),
        };

        data.and_then(|data| data.mimes.lock().ok().map(|mimes| mimes.clone()))
            .unwrap_or_default()
    }

    pub fn receive(&self, mime: &str, fd: BorrowedFd<'_>) {
        variant!(self, offer => offer.receive(mime.to_owned(), fd));
    }

    pub fn destroy(&self) {
        variant!(self, offer => offer.destroy());
    }
}

#[derive(Clone, Debug)]
pub enum Source {
    Ext(ExtDataControlSourceV1),
    Wlr(ZwlrDataControlSourceV1),
}

impl Source {
    pub fn offer(&self, mime: &str) {
        variant!(self, source => source.offer(mime.to_owned()));
    }

    pub fn destroy(&self) {
        variant!(self, source => source.destroy());
    }

    pub fn is(&self, id: &wayland_client::backend::ObjectId) -> bool {
        variant!(self, source => &source.id() == id)
    }

    pub fn data(&self) -> Option<&SourceData> {
        match self {
            Self::Ext(source) => source.data::<SourceData>(),
            Self::Wlr(source) => source.data::<SourceData>(),
        }
    }
}

#[derive(Debug)]
pub enum Selection {
    Announced,
    Current(Option<Offer>),
    Finished,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Runtime {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

wayland_client::delegate_noop!(Runtime: ignore WlSeat);
wayland_client::delegate_noop!(Runtime: ExtDataControlManagerV1);
wayland_client::delegate_noop!(Runtime: ZwlrDataControlManagerV1);

macro_rules! dispatch_family {
    (
        device: $device:ty, $device_mod:ident, $device_wrap:path;
        offer:  $offer:ty,  $offer_mod:ident,  $offer_wrap:path;
        source: $source:ty, $source_mod:ident;
    ) => {
        impl Dispatch<$device, ()> for Runtime {
            event_created_child!(Runtime, $device, [
                0 => ($offer, OfferData::default())
            ]);

            fn event(
                state: &mut Self,
                _: &$device,
                event: $device_mod::Event,
                _: &(),
                connection: &Connection,
                _: &QueueHandle<Self>,
            ) {
                let selection = match event {
                    $device_mod::Event::DataOffer { .. } => Selection::Announced,
                    $device_mod::Event::Selection { id } => {
                        Selection::Current(id.map($offer_wrap))
                    }
                    $device_mod::Event::Finished => Selection::Finished,
                    _ => return,
                };

                state.on_selection(selection, connection);
            }
        }

        impl Dispatch<$offer, OfferData> for Runtime {
            fn event(
                _: &mut Self,
                _: &$offer,
                event: $offer_mod::Event,
                data: &OfferData,
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                let $offer_mod::Event::Offer { mime_type } = event else {
                    return;
                };

                if let Ok(mut mimes) = data.mimes.lock() {
                    mimes.push(mime_type);
                };
            }
        }

        impl Dispatch<$source, SourceData> for Runtime {
            fn event(
                state: &mut Self,
                source: &$source,
                event: $source_mod::Event,
                data: &SourceData,
                _: &Connection,
                _: &QueueHandle<Self>,
            ) {
                match event {
                    $source_mod::Event::Send { mime_type, fd } => {
                        state.on_source_send(data, &mime_type, fd);
                    }
                    $source_mod::Event::Cancelled => state.on_source_cancelled(&source.id()),
                    _ => {}
                }
            }
        }
    };
}

dispatch_family! {
    device: ExtDataControlDeviceV1, ext_data_control_device_v1, Offer::Ext;
    offer:  ExtDataControlOfferV1,  ext_data_control_offer_v1,  Offer::Ext;
    source: ExtDataControlSourceV1, ext_data_control_source_v1;
}

dispatch_family! {
    device: ZwlrDataControlDeviceV1, zwlr_data_control_device_v1, Offer::Wlr;
    offer:  ZwlrDataControlOfferV1,  zwlr_data_control_offer_v1,  Offer::Wlr;
    source: ZwlrDataControlSourceV1, zwlr_data_control_source_v1;
}

pub fn bind_seat(globals: &GlobalList, qh: &QueueHandle<Runtime>) -> Result<WlSeat, SetupError> {
    globals
        .bind::<WlSeat, _, _>(qh, 1..=9, ())
        .map_err(|_| SetupError::NoSeat)
}
