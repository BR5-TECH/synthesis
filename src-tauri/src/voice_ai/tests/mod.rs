//! The tests of Voice AI. Each test talks to a real local server over real
//! sockets, so that the protocol of each dialect is what is tested.

mod activity_detector;
mod activity_model;
mod activity_worker;
mod audio;
mod capture;
mod chunks;
mod interim;
mod languages;
mod protocol;
mod speech;
mod speech_options;
mod support;
mod verify;
