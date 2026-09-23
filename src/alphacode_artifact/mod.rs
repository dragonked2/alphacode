//! # alphacode-artifact
//!
//! Professional Artifact & Report Intelligence System for Alphacode.
//!
//! This module provides a reusable system that takes structured information
//! produced by Alphacode's agents, skills, tools, research workflows,
//! security workflows, CTF workflows, and future capabilities and transforms
//! that information into professional, evidence-driven, interactive,
//! self-contained artifacts.

pub mod metadata;

pub use metadata::{ArtifactMetadata, ArtifactType, Classification, Confidentiality};
