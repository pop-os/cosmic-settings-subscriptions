// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;
use zbus::zvariant::OwnedObjectPath;

mod adapter;
mod device;
pub mod subscription;

pub use adapter::*;
pub use device::*;

#[derive(Clone, Debug)]
pub enum Event {
    AddedAdapter(OwnedObjectPath, Adapter),
    AddedDevice(OwnedObjectPath, Device),
    DBusError(String),
    DBusServiceUnknown,
    DeviceFailed(OwnedObjectPath),
    Ok,
    RemovedAdapter(OwnedObjectPath),
    RemovedDevice(OwnedObjectPath),
    SetAdapters(HashMap<OwnedObjectPath, Adapter>),
    SetDevices(HashMap<OwnedObjectPath, Device>),
    UpdatedAdapter(OwnedObjectPath, Vec<AdapterUpdate>),
    UpdatedDevice(OwnedObjectPath, Vec<DeviceUpdate>),
}

#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
pub enum Active {
    #[default]
    Disabled,
    Disabling,
    Enabling,
    Enabled,
}
