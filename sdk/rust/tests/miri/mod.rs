//! Miri memory safety test suite for RustMQ Rust SDK
//! 
//! This module contains memory safety tests for the Rust SDK components:
//! - Client connection management
//! - Producer and consumer operations
//! - Message serialization/deserialization
//! - Configuration handling
//! - Error handling
//! 
//! To run these tests:
//! ```bash
//! cd sdk/rust
//! cargo +nightly miri test --features miri-safe miri::
//! ```

#[cfg(miri)]
pub mod client_tests;