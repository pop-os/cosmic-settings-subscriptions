// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use futures::{SinkExt, StreamExt};
use std::{fmt::Debug, hash::Hash};
use upower_dbus::{BatteryType, DeviceProxy, UPowerProxy};

pub fn device_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced_futures::Subscription<DeviceDbusEvent> {
    iced_futures::subscription::channel(id, 50, move |mut output| async move {
        let mut state = State::Ready;

        loop {
            state = start_listening(state, &mut output).await;
        }
    })
}

#[derive(Debug)]
pub enum State {
    Ready,
    Waiting(UPowerProxy<'static>, DeviceProxy<'static>),
    Finished,
}

async fn display_device() -> zbus::Result<(UPowerProxy<'static>, DeviceProxy<'static>)> {
    let connection = zbus::Connection::system().await?;
    let upower: UPowerProxy<'_> = UPowerProxy::new(&connection).await?;
    let device_path = upower.get_display_device().await?;
    Ok((upower, device_path))
}

async fn start_listening(
    state: State,
    output: &mut futures::channel::mpsc::Sender<DeviceDbusEvent>,
) -> State {
    match state {
        State::Ready => {
            if let Ok((upower, device)) = display_device().await {
                if let Ok(devices) = upower.enumerate_devices().await {
                    let mut has_battery = false;
                    for device in devices {
                        let Ok(d) = DeviceProxy::builder(upower.inner().connection()).path(device)
                        else {
                            continue;
                        };
                        let Ok(d) = d.build().await else {
                            continue;
                        };
                        if d.type_().await == Ok(BatteryType::Battery)
                            && d.power_supply().await.unwrap_or_default()
                        {
                            has_battery = true;
                            break;
                        }
                    }
                    if !has_battery {
                        std::process::exit(0);
                    }
                }
                _ = output
                    .send(DeviceDbusEvent::Update {
                        on_battery: upower
                            .cached_on_battery()
                            .unwrap_or_default()
                            .unwrap_or_default(),
                        percent: device
                            .cached_percentage()
                            .unwrap_or_default()
                            .unwrap_or_default(),
                        time_to_empty: device
                            .cached_time_to_empty()
                            .unwrap_or_default()
                            .unwrap_or_default(),
                    })
                    .await;
                return State::Waiting(upower, device);
            }
            State::Finished
        }
        State::Waiting(upower, device) => {
            let mut stream = futures::stream_select!(
                upower.receive_on_battery_changed().await.map(|_| ()),
                device.receive_percentage_changed().await.map(|_| ()),
                device.receive_time_to_empty_changed().await.map(|_| ()),
            );
            match stream.next().await {
                Some(_) => {
                    _ = output
                        .send(DeviceDbusEvent::Update {
                            on_battery: upower
                                .cached_on_battery()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                            percent: device
                                .cached_percentage()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                            time_to_empty: device
                                .cached_time_to_empty()
                                .unwrap_or_default()
                                .unwrap_or_default(),
                        })
                        .await;

                    State::Waiting(upower, device)
                }
                None => State::Finished,
            }
        }
        State::Finished => futures::future::pending().await,
    }
}

#[derive(Debug, Clone)]
pub enum DeviceDbusEvent {
    Update {
        on_battery: bool,
        percent: f64,
        time_to_empty: i64,
    },
}