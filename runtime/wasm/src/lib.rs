extern crate bs58;
extern crate ethabi;
extern crate futures;
extern crate graph;
extern crate graph_runtime_derive;
extern crate hex;
extern crate pwasm_utils;
extern crate semver;
extern crate tiny_keccak;
extern crate wasmi;

mod asc_abi;
mod host;
mod module;
mod to_from;

/// Runtime-agnostic implementation of exports to WASM.
mod host_exports;

use graph::prelude::*;
use graph::web3::types::{Address, Transaction};

pub use self::host::{RuntimeHost, RuntimeHostBuilder, RuntimeHostConfig};

#[derive(Clone, Debug)]
pub(crate) struct UnresolvedContractCall {
    pub contract_name: String,
    pub contract_address: Address,
    pub function_name: String,
    pub function_args: Vec<ethabi::Token>,
}

#[derive(Debug)]
pub(crate) struct MappingContext {
    logger: Logger,
    block: Arc<EthereumBlock>,
    entity_operations: Vec<EntityOperation>,
}

/// Cloning an `EventHandlerContext` clones all its fields,
/// except the `entity_operations`, since they are an output
/// accumulator and are therefore initialized with an empty `Vec`
impl Clone for MappingContext {
    fn clone(&self) -> Self {
        Self {
            logger: self.logger.clone(),
            block: self.block.clone(),
            entity_operations: Vec::new(),
        }
    }
}
