// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use super::Event;
pub use cosmic_dbus_networkmanager::interface::enums::{
    ActiveConnectionState, DeviceState, DeviceType,
};

use cosmic_dbus_networkmanager::nm::NetworkManager;
use futures::{SinkExt, StreamExt};
use iced_futures::{self, subscription};
use std::{fmt::Debug, hash::Hash, sync::Arc};
use zbus::{zvariant::ObjectPath, Connection};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeviceInfo {
    pub path: ObjectPath<'static>,
    pub device_type: DeviceType,
    pub interface: String,
    pub state: DeviceState,
    pub active_connection: Option<(DeviceConnection, ActiveConnectionState)>,
    pub available_connections: Vec<DeviceConnection>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeviceConnection {
    pub path: ObjectPath<'static>,
    pub id: String,
    pub uuid: Arc<str>,
}

pub async fn list<'a>(
    conn: &'a zbus::Connection,
    device_type_filter: fn(DeviceType) -> bool,
) -> zbus::Result<Vec<DeviceInfo>> {
    let nm = NetworkManager::new(conn).await?;
    let devices = nm.devices().await?;

    let device_iter = devices.into_iter().map(|device| async move {
        let (interface, hw_address, device_type, state, available_connections) =
            futures::try_join!(
                device.interface(),
                device.hw_address(),
                device.device_type(),
                device.state(),
                device.available_connections()
            )
            .ok()?;

        if !device_type_filter(device_type) {
            return None;
        }

        if hw_address.is_empty() {
            return None;
        }

        let (active_connection, available_connections) = futures::join!(
            async {
                let connection = device.active_connection().await?;

                let (id, uuid, state) =
                    futures::try_join!(connection.id(), connection.uuid(), connection.state())?;

                Ok::<_, zbus::Error>((
                    DeviceConnection {
                        id,
                        uuid: Arc::from(uuid),
                        path: connection.inner().path().to_owned(),
                    },
                    state,
                ))
            },
            futures::stream::FuturesOrdered::from_iter(available_connections.into_iter().map(
                |conn| async move {
                    let path = conn.inner().path().to_owned();

                    let settings = conn.get_settings().await.ok()?;

                    let id = settings
                        .get("connection")?
                        .get("id")?
                        .downcast_ref::<String>()
                        .ok()?;

                    let uuid = settings["connection"]
                        .get("uuid")?
                        .downcast_ref::<String>()
                        .ok()?;

                    Some(DeviceConnection {
                        id,
                        uuid: Arc::from(uuid),
                        path,
                    })
                }
            ),)
            .filter_map(|res| async move { res })
            .collect::<Vec<_>>()
        );

        Some(DeviceInfo {
            path: device.inner().path().to_owned(),
            device_type,
            interface,
            state,
            active_connection: active_connection.ok(),
            available_connections,
        })
    });

    let devices_info = futures::stream::FuturesOrdered::from_iter(device_iter)
        .filter_map(|res| async move { res })
        .collect::<Vec<DeviceInfo>>()
        .await;

    Ok(devices_info)
}

pub fn subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
    has_popup: bool,
    conn: Connection,
) -> iced_futures::Subscription<Event> {
    subscription::channel((id, has_popup), 50, move |output| async move {
        watch(conn, has_popup, output).await;
        futures::future::pending().await
    })
}

pub async fn watch(
    conn: zbus::Connection,
    has_popup: bool,
    mut output: futures::channel::mpsc::Sender<Event>,
) {
    let mut state = State::Continue(conn);

    loop {
        state = start_listening(state, has_popup, &mut output).await;
    }
}

#[derive(Debug, Clone)]
pub enum State {
    Continue(Connection),
    Error,
}

async fn start_listening(
    state: State,
    has_popup: bool,
    output: &mut futures::channel::mpsc::Sender<Event>,
) -> State {
    let conn = match state {
        State::Continue(conn) => conn,
        State::Error => futures::future::pending().await,
    };
    let network_manager = match NetworkManager::new(&conn).await {
        Ok(n) => n,
        Err(why) => {
            tracing::error!(
                why = why.to_string(),
                "failed to connect to network_manager"
            );
            return State::Error;
        }
    };

    let mut devices_changed = network_manager.receive_devices_changed().await;

    let secs = if has_popup { 4 } else { 60 };

    while let (Some(_change), _) = futures::future::join(
        devices_changed.next(),
        tokio::time::sleep(tokio::time::Duration::from_secs(secs)),
    )
    .await
    {
        _ = output.send(Event::Devices).await;
    }

    State::Continue(conn)
}
