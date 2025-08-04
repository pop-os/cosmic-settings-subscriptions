// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

#[cfg(feature = "accessibility")]
pub mod accessibility;

#[cfg(feature = "cosmic_a11y_manager")]
pub mod cosmic_a11y_manager;

#[cfg(feature = "airplane_mode")]
pub mod airplane_mode;

#[cfg(feature = "network_manager")]
pub mod network_manager;

#[cfg(feature = "pipewire")]
pub mod pipewire;

#[cfg(feature = "pulse")]
pub mod pulse;

#[cfg(feature = "bluetooth")]
pub mod bluetooth;

#[cfg(feature = "settings_daemon")]
pub mod settings_daemon;

#[cfg(feature = "sound")]
pub mod sound;

#[cfg(feature = "upower")]
pub mod upower;
