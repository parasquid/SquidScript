#![cfg_attr(not(test), no_std)]

pub mod chunk;
pub mod error;
pub mod host;
pub mod limits;
pub mod program;
pub mod reader;
pub mod strings;
pub mod value;
pub mod vm;

pub(crate) mod bytecode;
pub(crate) mod model;
pub(crate) mod state;

#[cfg(test)]
mod tests;
