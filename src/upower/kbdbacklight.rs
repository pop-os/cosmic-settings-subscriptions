// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

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

async fn get_brightness(kbd_proxy: &KbdBacklightProxy<'_>) -> zbus::Result<f64> {
    Ok(kbd_proxy.get_brightness().await? as f64 / kbd_proxy.get_max_brightness().await? as f64)
}

async fn events() -> zbus::Result<impl Stream<Item = KeyboardBacklightUpdate>> {
    let conn = zbus::Connection::system().await?;
    let kbd_proxy = KbdBacklightProxy::builder(&conn).build().await?;
    let (tx, rx) = unbounded_channel();

    let b = get_brightness(&kbd_proxy).await.ok();

    let brightness_changed_stream = kbd_proxy.receive_brightness_changed().await?;

    let initial = futures::stream::iter([
        KeyboardBacklightUpdate::Sender(tx),
        KeyboardBacklightUpdate::Brightness(b),
    ]);
    let stream = futures::stream::select(
        UnboundedReceiverStream::new(rx).map(Event::Request),
        brightness_changed_stream.map(Event::BrightnessChanged),
    );
    Ok(initial.chain(stream.filter_map(move |event| {
        let kbd_proxy = kbd_proxy.clone();
        async move {
            match event {
                Event::BrightnessChanged(_changed) => {
                    let b = get_brightness(&kbd_proxy).await.ok();
                    Some(KeyboardBacklightUpdate::Brightness(b))
                }
                Event::Request(req) => match req {
                    KeyboardBacklightRequest::Get => {
                        let b = get_brightness(&kbd_proxy).await.ok();
                        Some(KeyboardBacklightUpdate::Brightness(b))
                    }
                    KeyboardBacklightRequest::Set(value) => {
                        if let Ok(max_brightness) = kbd_proxy.get_max_brightness().await {
                            let value = value.clamp(0., 1.) * (max_brightness as f64);
                            let value = value.round() as i32;
                            let _ = kbd_proxy.set_brightness(value).await;
                        }
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
    // TODO: Send brightness and max brightness as integers
    Brightness(Option<f64>),
}

#[derive(Debug, Clone)]
pub enum KeyboardBacklightRequest {
    Get,
    Set(f64),
}
