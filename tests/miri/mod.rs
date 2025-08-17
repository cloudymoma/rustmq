//! Miri memory safety test suite for RustMQ
//! 
//! This module contains comprehensive memory safety tests that run under Miri
//! to detect undefined behavior, data races, and memory safety violations.
//! 
//! These tests focus on the core components of RustMQ:
//! - Storage layer (WAL, object storage, buffer pools)
//! - Security layer (authentication, authorization, ACL)
//! - Cache layer (both Moka and LRU implementations)
//! 
//! To run these tests:
//! ```bash
//! cargo +nightly miri test --features miri-safe miri::
//! ```

#[cfg(miri)]
pub mod storage_tests;

#[cfg(miri)]
pub mod security_tests;

#[cfg(miri)]
pub mod cache_tests;

#[cfg(miri)]
pub mod proptest_integration;