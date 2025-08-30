// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

// XXX error handling?

use futures::{FutureExt, StreamExt};
use iced_futures::Subscription;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio_stream::wrappers::UnboundedReceiverStream;

pub fn subscription(connection: zbus::Connection) -> iced_futures::Subscription<Event> {
    Subscription::run_with_id(
        "settings-daemon",
        async move {
            let settings_daemon = match CosmicSettingsDaemonProxy::new(&connection).await {
                Ok(value) => value,
                Err(err) => {
                    log::error!("Error connecting to settings daemon: {}", err);
                    futures::future::pending().await
                }
            };

            let (tx, rx) = unbounded_channel();

            let max_brightness_stream = settings_daemon
                .receive_max_display_brightness_changed()
                .await;
            let brightness_stream = settings_daemon.receive_display_brightness_changed().await;
            let owner_stream = settings_daemon.0.receive_owner_changed().await.unwrap();

            let initial = futures::stream::iter([Event::Sender(tx)]);

            let settings_daemon_clone = settings_daemon.clone();
            initial.chain(futures::stream_select!(
                Box::pin(UnboundedReceiverStream::new(rx).filter_map(move |request| {
                    let settings_daemon = settings_daemon.clone();
                    async move {
                        match request {
                            Request::SetDisplayBrightness(brightness) => {
                                let _ = settings_daemon.set_display_brightness(brightness).await;
                            }
                        }
                        None::<Event>
                    }
                })),
                Box::pin(max_brightness_stream.filter_map(|evt| async move {
                    Some(Event::MaxDisplayBrightness(evt.get().await.ok()?))
                })),
                Box::pin(brightness_stream.filter_map(|evt| async move {
                    Some(Event::DisplayBrightness(evt.get().await.ok()?))
                })),
                Box::pin(owner_stream.flat_map(move |owner| {
                    let settings_daemon = settings_daemon_clone.clone();
                    async move {
                        // TODO send None if owner is lost? Needs to be well-ordered. Or tracked
                        // seperately from brightness.
                        if owner.is_some() {
                            return futures::stream::iter(
                                [
                                    settings_daemon
                                        .display_brightness()
                                        .await
                                        .ok()
                                        .map(Event::DisplayBrightness),
                                    settings_daemon
                                        .max_display_brightness()
                                        .await
                                        .ok()
                                        .map(Event::MaxDisplayBrightness),
                                ]
                                .into_iter()
                                .flatten(),
                            )
                            .left_stream();
                        }
                        futures::stream::empty().right_stream()
                    }
                    .flatten_stream()
                })),
            ))
        }
        .flatten_stream(),
    )
}

#[derive(Clone, Debug)]
pub enum Event {
    Sender(UnboundedSender<Request>),
    MaxDisplayBrightness(i32),
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
    fn set_display_brightness(&self, value: i32) -> zbus::Result<()>;
    #[zbus(property)]
    fn max_display_brightness(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn keyboard_brightness(&self) -> zbus::Result<i32>;
}

#[derive(Debug, Clone)]
pub enum Request {
    SetDisplayBrightness(i32),
}
