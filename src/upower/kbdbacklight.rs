// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

// TODO Test how this handles upower starting after applet does, if it ever is
// started or restarted.

use futures::{FutureExt, Stream, StreamExt};
use std::{fmt::Debug, hash::Hash};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
use upower_dbus::{BrightnessChanged, KbdBacklightProxy};

pub fn kbd_backlight_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced_futures::Subscription<KeyboardBacklightUpdate> {
    iced_futures::subscription::run_with_id(
        id,
        async move {
            match events().await {
                Ok(stream) => stream,
                Err(err) => {
                    log::error!("Error listening to KbdBacklight: {}", err);
                    futures::future::pending().await
                }
            }
        }
        .flatten_stream(),
    )
}

enum Event {
    BrightnessChanged(BrightnessChanged),
    Request(KeyboardBacklightRequest),
}

async fn events() -> zbus::Result<impl Stream<Item = KeyboardBacklightUpdate>> {
    let conn = zbus::Connection::system().await?;
    let kbd_proxy = KbdBacklightProxy::builder(&conn).build().await?;
    let (tx, rx) = unbounded_channel();

    let max_brightness = kbd_proxy.get_max_brightness().await?;
    let brightness = kbd_proxy.get_brightness().await?;

    let brightness_changed_stream = kbd_proxy.receive_brightness_changed().await?;

    let initial = futures::stream::iter([
        KeyboardBacklightUpdate::Sender(tx),
        KeyboardBacklightUpdate::MaxBrightness(max_brightness),
        KeyboardBacklightUpdate::Brightness(brightness),
    ]);
    let stream = futures::stream::select(
        UnboundedReceiverStream::new(rx).map(Event::Request),
        brightness_changed_stream.map(Event::BrightnessChanged),
    );
    Ok(initial.chain(stream.filter_map(move |event| {
        let kbd_proxy = kbd_proxy.clone();
        async move {
            match event {
                Event::BrightnessChanged(changed) => {
                    if let Ok(args) = changed.args() {
                        Some(KeyboardBacklightUpdate::Brightness(*args.value()))
                    } else {
                        None
                    }
                }
                Event::Request(req) => match req {
                    KeyboardBacklightRequest::Get => {
                        if let Ok(brightness) = kbd_proxy.get_brightness().await {
                            Some(KeyboardBacklightUpdate::Brightness(brightness))
                        } else {
                            None
                        }
                    }
                    KeyboardBacklightRequest::Set(value) => {
                        let _ = kbd_proxy.set_brightness(value).await;
                        None
                    }
                },
            }
        }
    })))
}

#[derive(Debug, Clone)]
pub enum KeyboardBacklightUpdate {
    Sender(UnboundedSender<KeyboardBacklightRequest>),
    Brightness(i32),
    MaxBrightness(i32),
}

#[derive(Debug, Clone)]
pub enum KeyboardBacklightRequest {
    Get,
    Set(i32),
}
