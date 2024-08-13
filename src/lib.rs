// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

#[cfg(feature = "airplane_mode")]
pub mod airplane_mode;

#[cfg(feature = "pipewire")]
pub mod pipewire;

#[cfg(feature = "pulse")]
pub mod pulse;

#[cfg(feature = "settings_daemon")]
pub mod settings_daemon;

#[cfg(feature = "upower")]
pub mod upower;
