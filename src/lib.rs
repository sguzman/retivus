//! Core import and provider interfaces, independent of the GUI.
pub mod provider;

pub mod brand;

pub mod source_adapter;

pub mod preview;

#[cfg(feature = "anki-backend")]
pub mod anki_backend;
