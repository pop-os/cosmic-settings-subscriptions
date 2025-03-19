// Copyright 2024 System76 <info@system76.com>
// SPDX-License-Identifier: MPL-2.0

// Make sure not to fail if pulse not found, and reconnect?
// change to device shouldn't send osd?

use futures::{executor::block_on, SinkExt};
use iced_futures::{stream, Subscription};
use libpulse_binding::{
    callbacks::ListResult,
    context::{
        introspect::{CardInfo, CardProfileInfo, Introspector, ServerInfo, SinkInfo, SourceInfo},
        subscribe::{Facility, InterestMaskSet, Operation},
        Context, FlagSet, State,
    },
    def::Retval,
    mainloop::standard::{IterateResult, Mainloop},
    volume::Volume,
};
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    rc::Rc,
};

pub fn subscription() -> iced_futures::Subscription<Event> {
    Subscription::run_with_id(
        "pulse",
        stream::channel(20, |sender| async {
            std::thread::spawn(move || thread(sender));
            futures::future::pending().await
        }),
    )
}

pub fn thread(sender: futures::channel::mpsc::Sender<Event>) {
    let mut started = false;

    loop {
        let Some(mut main_loop) = Mainloop::new() else {
            log::error!("Failed to create PA main loop");
            return;
        };

        let Some(mut context) = Context::new(&main_loop, "cosmic-osd") else {
            log::error!("Failed to create PA context");
            return;
        };

        let data = Rc::new(Data {
            main_loop: RefCell::new(Mainloop {
                _inner: Rc::clone(&main_loop._inner),
            }),
            introspector: context.introspect(),
            sink_volume: Cell::new(None),
            sink_mute: Cell::new(None),
            source_volume: Cell::new(None),
            source_mute: Cell::new(None),
            default_sink_name: RefCell::new(None),
            default_source_name: RefCell::new(None),
            sender: RefCell::new(sender.clone()),
        });

        let data_clone = data.clone();
        context.set_subscribe_callback(Some(Box::new(move |facility, operation, index| {
            data_clone.subscribe_cb(facility.unwrap(), operation, index);
        })));

        let _ = context.connect(None, FlagSet::NOFAIL, None);

        loop {
            if sender.is_closed() {
                return;
            }

            match main_loop.iterate(false) {
                IterateResult::Success(_) => {}
                IterateResult::Err(_e) => {
                    return;
                }
                IterateResult::Quit(_e) => {
                    return;
                }
            }

            if context.get_state() == State::Ready {
                if !started {
                    started = true;

                    // Inspect all available cards on startup
                    data.introspector.get_card_info_list({
                        let data_weak = Rc::downgrade(&data);
                        move |card_info_res| {
                            if let Some(data) = data_weak.upgrade() {
                                data.card_info_cb(card_info_res)
                            }
                        }
                    });
                }
                break;
            }
        }

        data.get_server_info();
        context.subscribe(
            InterestMaskSet::SERVER | InterestMaskSet::SINK | InterestMaskSet::SOURCE,
            |_| {},
        );

        if let Err((err, retval)) = main_loop.run() {
            log::error!("PA main loop returned {:?}, error {}", retval, err);
        }
    }
}

#[derive(Clone, Debug)]
pub enum Event {
    CardInfo(Card),
    DefaultSink(String),
    DefaultSource(String),
    SinkVolume(u32),
    SinkMute(bool),
    SourceVolume(u32),
    SourceMute(bool),
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct Card {
    pub object_id: u32,
    pub name: String,
    pub variant: DeviceVariant,
    pub ports: Vec<CardPort>,
    pub profiles: Vec<CardProfile>,
    pub active_profile: Option<CardProfile>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct CardPort {
    pub name: String,
    pub description: String,
    pub direction: Direction,
    pub port_type: PortType,
    pub profile_port: u32,
    pub priority: u32,
    pub profiles: Vec<CardProfile>,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct CardProfile {
    pub name: String,
    pub description: String,
    pub available: bool,
    pub n_sinks: u32,
    pub n_sources: u32,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum DeviceVariant {
    Alsa {
        alsa_card: u32,
        alsa_card_name: String,
        card_profile_device: u32,
    },
    Bluez5 {
        address: String,
        codec: String,
        profile: String,
    },
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum Direction {
    Input,
    Output,
    Both,
}

#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub enum PortType {
    Analog,
    Digital,
    Unknown,
}

struct Data {
    main_loop: RefCell<Mainloop>,
    default_sink_name: RefCell<Option<String>>,
    default_source_name: RefCell<Option<String>>,
    sink_volume: Cell<Option<u32>>,
    sink_mute: Cell<Option<bool>>,
    source_volume: Cell<Option<u32>>,
    source_mute: Cell<Option<bool>>,
    introspector: Introspector,
    sender: RefCell<futures::channel::mpsc::Sender<Event>>,
}

impl Data {
    fn card_info_cb(self: &Rc<Self>, card_info: ListResult<&CardInfo>) {
        if let ListResult::Item(card_info) = card_info {
            let Some(object_id) = card_info
                .proplist
                .get_str("object.id")
                .and_then(|v| v.parse::<u32>().ok())
            else {
                return;
            };

            let variant = if let Some(alsa_card) = card_info
                .proplist
                .get_str("alsa.card")
                .and_then(|v| v.parse::<u32>().ok())
            {
                DeviceVariant::Alsa {
                    alsa_card,
                    alsa_card_name: card_info
                        .proplist
                        .get_str("alsa.card_name")
                        .unwrap_or_default(),
                    card_profile_device: card_info
                        .proplist
                        .get_str("card.profile.device")
                        .and_then(|v| v.parse::<u32>().ok())
                        .unwrap_or_default(),
                }
            } else if let Some(address) = card_info.proplist.get_str("api.bluez5.address") {
                DeviceVariant::Bluez5 {
                    address,
                    codec: card_info
                        .proplist
                        .get_str("api.bluez5.codec")
                        .unwrap_or_default(),
                    profile: card_info
                        .proplist
                        .get_str("api.bluez5.profile")
                        .unwrap_or_default(),
                }
            } else {
                return;
            };

            let card = Card {
                name: card_info
                    .name
                    .as_ref()
                    .map(Cow::to_string)
                    .unwrap_or_default(),
                object_id,
                variant,
                ports: card_info
                    .ports
                    .iter()
                    .map(|port| CardPort {
                        name: port.name.as_ref().map(Cow::to_string).unwrap_or_default(),
                        description: port
                            .description
                            .as_ref()
                            .map(Cow::to_string)
                            .unwrap_or_default(),
                        direction: match port.direction.bits() {
                            x if x == libpulse_binding::direction::FlagSet::INPUT.bits() => Direction::Input,
                            x if x == libpulse_binding::direction::FlagSet::OUTPUT.bits() => Direction::Output,
                            _ => Direction::Both
                        },
                        port_type: match port.proplist.get_str("port.type").as_deref() {
                            Some("analog") => PortType::Analog,
                            Some("digital") => PortType::Digital,
                            _ => PortType::Unknown,
                        },
                        profile_port: port
                            .proplist
                            .get_str("card.profile.port")
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(0),
                        priority: port.priority,
                        profiles: collect_profiles(&port.profiles),
                    })
                    .collect(),
                profiles: collect_profiles(&card_info.profiles),
                active_profile: card_info.active_profile.as_deref().map(CardProfile::from),
            };

            if block_on(self.sender.borrow_mut().send(Event::CardInfo(card))).is_err() {
                self.main_loop.borrow_mut().quit(Retval(0));
            }
        }
    }

    fn server_info_cb(self: &Rc<Self>, server_info: &ServerInfo) {
        let new_default_sink_name = server_info
            .default_sink_name
            .as_ref()
            .map(|x| x.clone().into_owned());
        let mut default_sink_name = self.default_sink_name.borrow_mut();
        if new_default_sink_name != *default_sink_name {
            if let Some(name) = &new_default_sink_name {
                _ = block_on(
                    self.sender
                        .borrow_mut()
                        .send(Event::DefaultSink(name.clone())),
                );
                self.get_sink_info_by_name(name);
            }
            *default_sink_name = new_default_sink_name;
        }

        let new_default_source_name = server_info
            .default_source_name
            .as_ref()
            .map(|x| x.clone().into_owned());
        let mut default_source_name = self.default_source_name.borrow_mut();
        if new_default_source_name != *default_source_name {
            if let Some(name) = &new_default_source_name {
                _ = block_on(
                    self.sender
                        .borrow_mut()
                        .send(Event::DefaultSource(name.clone())),
                );
                self.get_source_info_by_name(name);
            }
            *default_source_name = new_default_source_name;
        }
    }

    fn get_server_info(self: &Rc<Self>) {
        let data = self.clone();
        self.introspector
            .get_server_info(move |server_info| data.server_info_cb(server_info));
    }

    fn sink_info_cb(&self, sink_info_res: ListResult<&SinkInfo>) {
        if let ListResult::Item(sink_info) = sink_info_res {
            if sink_info.name.as_deref() != self.default_sink_name.borrow().as_deref() {
                return;
            }
            let volume = sink_info.volume.avg().0 / (Volume::NORMAL.0 / 100);
            if self.sink_mute.get() != Some(sink_info.mute) {
                self.sink_mute.set(Some(sink_info.mute));
                if block_on(
                    self.sender
                        .borrow_mut()
                        .send(Event::SinkMute(sink_info.mute)),
                )
                .is_err()
                {
                    self.main_loop.borrow_mut().quit(Retval(0));
                }
            }
            if self.sink_volume.get() != Some(volume) {
                self.sink_volume.set(Some(volume));
                if block_on(self.sender.borrow_mut().send(Event::SinkVolume(volume))).is_err() {
                    self.main_loop.borrow_mut().quit(Retval(0));
                }
            }
        }
    }

    fn source_info_cb(&self, source_info_res: ListResult<&SourceInfo>) {
        if let ListResult::Item(source_info) = source_info_res {
            if source_info.name.as_deref() != self.default_source_name.borrow().as_deref() {
                return;
            }
            let volume = source_info.volume.avg().0 / (Volume::NORMAL.0 / 100);
            if self.source_mute.get() != Some(source_info.mute) {
                self.source_mute.set(Some(source_info.mute));
                if block_on(
                    self.sender
                        .borrow_mut()
                        .send(Event::SourceMute(source_info.mute)),
                )
                .is_err()
                {
                    self.main_loop.borrow_mut().quit(Retval(0));
                }
            }
            if self.source_volume.get() != Some(volume) {
                self.source_volume.set(Some(volume));
                if block_on(self.sender.borrow_mut().send(Event::SourceVolume(volume))).is_err() {
                    self.main_loop.borrow_mut().quit(Retval(0));
                }
            }
        }
    }

    fn get_card_info_by_index(self: &Rc<Self>, index: u32) {
        let data = self.clone();
        self.introspector
            .get_card_info_by_index(index, move |card_info_res| {
                data.card_info_cb(card_info_res);
            });
    }

    fn get_sink_info_by_index(self: &Rc<Self>, index: u32) {
        let data = self.clone();
        self.introspector
            .get_sink_info_by_index(index, move |sink_info_res| {
                data.sink_info_cb(sink_info_res);
            });
    }

    fn get_sink_info_by_name(self: &Rc<Self>, name: &str) {
        let data = self.clone();
        self.introspector
            .get_sink_info_by_name(name, move |sink_info_res| {
                data.sink_info_cb(sink_info_res);
            });
    }

    fn get_source_info_by_index(self: &Rc<Self>, index: u32) {
        let data = self.clone();
        self.introspector
            .get_source_info_by_index(index, move |source_info_res| {
                data.source_info_cb(source_info_res);
            });
    }

    fn get_source_info_by_name(self: &Rc<Self>, name: &str) {
        let data = self.clone();
        self.introspector
            .get_source_info_by_name(name, move |source_info_res| {
                data.source_info_cb(source_info_res);
            });
    }

    fn subscribe_cb(
        self: &Rc<Self>,
        facility: Facility,
        _operation: Option<Operation>,
        index: u32,
    ) {
        match facility {
            Facility::Server => {
                self.get_server_info();
            }
            Facility::Sink => {
                self.get_sink_info_by_index(index);
            }
            Facility::Source => {
                self.get_source_info_by_index(index);
            }
            Facility::Card => {
                self.get_card_info_by_index(index);
            }
            _ => {}
        }
    }
}

fn collect_profiles(profiles: &[CardProfileInfo]) -> Vec<CardProfile> {
    profiles.iter().map(CardProfile::from).collect()
}

impl<'a> From<&CardProfileInfo<'a>> for CardProfile {
    fn from(profile: &CardProfileInfo) -> Self {
        CardProfile {
            name: profile
                .name
                .as_ref()
                .map(Cow::to_string)
                .unwrap_or_default(),
            description: profile
                .description
                .as_ref()
                .map(Cow::to_string)
                .unwrap_or_default(),
            available: profile.available,
            n_sinks: profile.n_sinks,
            n_sources: profile.n_sources,
        }
    }
}
