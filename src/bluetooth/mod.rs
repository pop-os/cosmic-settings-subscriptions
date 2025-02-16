// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;
use zbus::zvariant::OwnedObjectPath;

mod device;

pub use device::*;

#[derive(Clone, Debug)]
pub enum Event {
    DBusError(String),
    DeviceFailed(OwnedObjectPath),
    Ok,
    SetDevices(HashMap<OwnedObjectPath, Device>),
}

#[derive(Default, Debug, Clone, Copy, Eq, PartialEq)]
pub enum Active {
    #[default]
    Disabled,
    Disabling,
    Enabling,
    Enabled,
}
