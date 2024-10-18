// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

use std::collections::HashMap;

mod rfkill;

use futures::{FutureExt, StreamExt};
use iced_futures::Subscription;

pub fn subscription() -> iced_futures::Subscription<bool> {
    Subscription::run_with_id(
        "airplane-mode",
        async {
            match rfkill::rfkill_updates() {
                Ok(updates) => updates.filter_map(|state| async {
                    match state {
                        Ok(state) => Some(is_airplane_mode(&state)),
                        Err(err) => {
                            log::error!("Failed to read rfkill: {}", err);
                            None
                        }
                    }
                }),
                Err(err) => {
                    log::error!("Failed to monitor rfkill: {}", err);
                    futures::future::pending().await
                }
            }
        }
        .flatten_stream(),
    )
}

// Test that:
// - There is at least one device
// - All devices have either a hard or soft block active
fn is_airplane_mode(rfkill_state: &HashMap<u32, rfkill::DeviceState>) -> bool {
    !rfkill_state.is_empty()
        && rfkill_state
            .values()
            .all(|device_state| device_state.hard || device_state.soft)
}
