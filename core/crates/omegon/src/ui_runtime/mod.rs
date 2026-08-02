//! Shared UI runtime contracts for frontend adapters.
//!
//! This module is the inbound half of the InterfaceBoundary Contract:
//! frontend adapters may couple tightly to these semantic actions/outcomes, but
//! the contract must stay independent of renderer crates and backend internals.
//! `surfaces` remain the outbound semantic state contract.

pub mod actions;
pub mod envelope;
pub mod replay;
pub mod revision;
