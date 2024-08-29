// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use cosmic_dbus_networkmanager::{device::wireless::WirelessDevice, interface::enums::DeviceState};

use futures::StreamExt;
use itertools::Itertools;
use std::{collections::HashMap, sync::Arc};
use zbus::zvariant::ObjectPath;

pub async fn handle_wireless_device(device: WirelessDevice<'_>) -> zbus::Result<Vec<AccessPoint>> {
    device.request_scan(HashMap::new()).await?;

    let mut scan_changed = device.receive_last_scan_changed().await;

    if let Some(t) = scan_changed.next().await {
        match t.get().await {
            Ok(-1) => {
                tracing::error!("wireless device scan errored");
                return Ok(Default::default());
            }

            _ => (),
        }
    }

    let access_points = device.get_access_points().await?;

    let state: DeviceState = device
        .upcast()
        .await
        .and_then(|dev| dev.cached_state())
        .unwrap_or_default()
        .map(|s| s.into())
        .unwrap_or_else(|| DeviceState::Unknown);

    // Sort by strength and remove duplicates
    let mut aps = HashMap::<String, AccessPoint>::new();
    for ap in access_points {
        let (ssid_res, strength_res) = futures::join!(ap.ssid(), ap.strength());

        if let Some((ssid, strength)) = ssid_res.ok().zip(strength_res.ok()) {
            let ssid = String::from_utf8_lossy(&ssid.clone()).into_owned();
            if let Some(access_point) = aps.get(&ssid) {
                if access_point.strength > strength {
                    continue;
                }
            }

            aps.insert(
                ssid.clone(),
                AccessPoint {
                    ssid: Arc::from(ssid),
                    strength,
                    state,
                    working: false,
                    path: ap.inner().path().to_owned(),
                },
            );
        }
    }

    let aps = aps
        .into_values()
        .sorted_by(|a, b| b.strength.cmp(&a.strength))
        .collect();

    Ok(aps)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
    pub ssid: Arc<str>,
    pub strength: u8,
    pub state: DeviceState,
    pub working: bool,
    pub path: ObjectPath<'static>,
}
