//! BenchHub Core Library
//!
//! Benchmark execution engine, system telemetry, and result management library.
//!
//! # Modules
//!
//! - `bench_config`: Parsing and defining specific benchmarks.
//! - `config`: Application configuration and settings parsing.
//! - `db`: SQLite database interaction for run history and metadata.
//! - `engine`: Benchmark execution engine separated into `platform_env`, core runner, and per-benchmark `adapters`.
//! - `eula`: Official End User License Agreements (EULA) and software terms.
//! - `export`: Export functionalities to SVG, CSV, PNG, etc.
//! - `logging`: Application-level and run-level logging logic.
//! - `models`: Shared domain models and DTOs.
//! - `service`: Service layer orchestrating database, engine, and profiles.
//! - `telemetry`: System telemetry and hardware info (CPU, GPU, RAM, power).
//! - `thermal`: Thermal conditioning routines before benchmark execution.
//! - `utils`: Miscellaneous helper functions and string manipulation.

pub mod bench_config;
pub mod config;
pub mod db;
pub mod engine;
pub mod eula;
pub mod export;
pub mod logging;
pub mod models;
pub mod service;
pub mod telemetry;
pub mod thermal;
pub mod utils;
