// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

// XXX error handling?

use futures::{FutureExt, StreamExt};

pub fn subscription(connection: zbus::Connection) -> iced_futures::Subscription<Event> {
    iced_futures::subscription::run_with_id(
        "settings-daemon",
        async move {
            let settings_daemon = match CosmicSettingsDaemonProxy::new(&connection).await {
                Ok(value) => value,
                Err(_err) => futures::future::pending().await,
            };
            let disp_stream = settings_daemon
                .receive_display_brightness_changed()
                .await
                .filter_map(
                    |evt| async move { Some(Event::DisplayBrightness(evt.get().await.ok()?)) },
                );
            disp_stream
        }
        .flatten_stream(),
    )
}

#[derive(Clone, Debug)]
pub enum Event {
    DisplayBrightness(i32),
}

#[zbus::proxy(
    default_service = "com.system76.CosmicSettingsDaemon",
    interface = "com.system76.CosmicSettingsDaemon",
    default_path = "/com/system76/CosmicSettingsDaemon"
)]
trait CosmicSettingsDaemon {
    #[zbus(property)]
    fn display_brightness(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn keyboard_brightness(&self) -> zbus::Result<i32>;
}
