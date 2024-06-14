// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use futures::{SinkExt, StreamExt};
use std::{fmt::Debug, hash::Hash};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_stream::wrappers::UnboundedReceiverStream;
use upower_dbus::{BrightnessChanged, KbdBacklightProxy};

pub fn kbd_backlight_subscription<I: 'static + Hash + Copy + Send + Sync + Debug>(
    id: I,
) -> iced_futures::Subscription<KeyboardBacklightUpdate> {
    iced_futures::subscription::channel(id, 50, move |mut output| async move {
        if let Err(err) = listen(&mut output).await {
            log::error!("Error listening to KbdBacklight: {}", err);
        }

        futures::future::pending().await
    })
}

#[derive(Debug)]
pub enum State {
    Ready,
    Waiting(
        KbdBacklightProxy<'static>,
        UnboundedReceiver<KeyboardBacklightRequest>,
    ),
    Finished,
}

enum Event {
    BrightnessChanged(BrightnessChanged),
    Request(KeyboardBacklightRequest),
}

async fn get_brightness(kbd_proxy: &KbdBacklightProxy<'_>) -> zbus::Result<f64> {
    Ok(kbd_proxy.get_brightness().await? as f64 / kbd_proxy.get_max_brightness().await? as f64)
}

async fn listen(
    output: &mut futures::channel::mpsc::Sender<KeyboardBacklightUpdate>,
) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let kbd_proxy = KbdBacklightProxy::builder(&conn).build().await?;
    let (tx, rx) = unbounded_channel();

    let b = get_brightness(&kbd_proxy).await.ok();
    _ = output.send(KeyboardBacklightUpdate::Sender(tx)).await;
    _ = output.send(KeyboardBacklightUpdate::Brightness(b)).await;

    let brightness_changed_stream = kbd_proxy.receive_brightness_changed().await?;

    let mut stream = futures::stream::select(
        UnboundedReceiverStream::new(rx).map(Event::Request),
        brightness_changed_stream.map(Event::BrightnessChanged),
    );
    while let Some(event) = stream.next().await {
        match event {
            Event::BrightnessChanged(_changed) => {
                let b = get_brightness(&kbd_proxy).await.ok();
                _ = output.send(KeyboardBacklightUpdate::Brightness(b)).await;
            }
            Event::Request(req) => match req {
                KeyboardBacklightRequest::Get => {
                    let b = get_brightness(&kbd_proxy).await.ok();
                    _ = output.send(KeyboardBacklightUpdate::Brightness(b)).await;
                }
                KeyboardBacklightRequest::Set(value) => {
                    if let Ok(max_brightness) = kbd_proxy.get_max_brightness().await {
                        let value = value.clamp(0., 1.) * (max_brightness as f64);
                        let value = value.round() as i32;
                        let _ = kbd_proxy.set_brightness(value).await;
                    }
                }
            },
        }
    }

    Ok(())
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
