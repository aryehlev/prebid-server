//! # experimental-ml
//!
//! **EXPERIMENTAL** machine-learning helpers for Prebid Server's Rust port.
//!
//! This crate provides lightweight, pure-Rust ML primitives for three use cases:
//!
//! - **Floor prediction** — an MLP that estimates a reasonable floor price
//!   given a feature vector extracted from an incoming auction request.
//! - **Bid shading** — a linear model that suggests a shaded bid for
//!   first-price auctions.
//! - **Timeout prediction** — per-bidder EWMA statistics for suggesting
//!   adaptive per-bidder timeouts.
//!
//! The crate deliberately avoids heavy ML frameworks (no `candle`, no `torch`,
//! no `onnxruntime`) in order to keep compile times and the dependency
//! footprint small. All math is done on top of [`ndarray`] with a tiny
//! hand-rolled LCG PRNG for weight initialisation.
//!
//! All APIs in this crate are considered unstable and may change at any time.

pub mod error;
pub mod features;
pub mod models;
pub mod runner;

pub use error::MlError;
pub use features::{BasicFeatureExtractor, FeatureExtractor, FeatureVector, FEATURE_DIM};
pub use models::bid_shader::BidShader;
pub use models::floor_predictor::FloorPredictor;
pub use models::timeout_predictor::TimeoutPredictor;
pub use runner::{Prediction, PredictionPipeline};
