//! Independently implemented HDS200 transport, bounded controls and raw waveform model.
pub mod autoset;
pub mod control;
pub mod generator;
pub mod protocol;
pub mod trigger;
pub mod usb;
pub mod waveform;
pub type Result<T> = std::result::Result<T, String>;
