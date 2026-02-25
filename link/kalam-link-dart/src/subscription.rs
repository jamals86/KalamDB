//! Subscription helpers.
//!
//! The main subscription logic lives in `api::dart_subscribe` which uses
//! `StreamSink<DartChangeEvent>` to push events into a Dart Stream.
//! This module is reserved for future subscription utilities (e.g.,
//! reconnection strategies, buffering policies).
