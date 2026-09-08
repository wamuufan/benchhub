//! BenchHub
//!
//! A comprehensive benchmarking tool and result manager.
//!
//! # Modules
//!
//! - `config`: Application configuration and settings parsing.
//! - `db`: SQLite database interaction for run history and metadata.
//! - `engine`: Benchmark execution engine, runner logic, and metric tracking.
//! - `export`: Export functionalities to SVG, CSV, etc.
//! - `telemetry`: System telemetry and hardware info (CPU, GPU, RAM, power).
//! - `logging`: Application-level and run-level logging logic.
//! - `i18n`: Internationalization and translation services.
//! - `utils`: Miscellaneous helper functions and string manipulation.
//! - `bench_config`: Parsing and defining specific benchmarks.
//! - `eula`: Official End User License Agreements (EULA) and software terms.

pub mod bench_config;
pub mod config;
pub mod db;
pub mod engine;
pub mod eula;
pub mod export;
pub mod i18n;
pub mod logging;
pub mod models;
pub mod service;
pub mod telemetry;
pub mod utils;
