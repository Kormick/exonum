// Copyright 2019 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![deny(missing_docs)]

//! Helper crate for secure and convenient configuration of the Exonum nodes.
//!
//! `exonum-cli` supports multi-stage configuration process made with safety in mind. It involves
//! 4 steps (or stages) and allows to configure and run multiple blockchain nodes without
//! need in exchanging private keys between administrators.
//!
//! # How to Run the Network
//!
//! 1. Generate common (template) part of the nodes configuration using `generate-template` command.
//!   Generated `.toml` file must be spread among all the nodes and must be used in the following
//!   configuration step.
//! 2. Generate public and secret (private) parts of the node configuration using `generate-config`
//!   command. At this step, Exonum will generate master key from which consensus and service
//!   validator keys are derived. Master key is stored in the encrypted file. Consensus secret key
//!   is used for communications between the nodes, while service secret key is used
//!   mainly to sign transactions generated by the node. Both secret keys may be encrypted with a
//!   password. The public part of the node configuration must be spread among all nodes, while the
//!   secret part must be only accessible by the node administrator only.
//! 3. Generate final node configuration using `finalize` command. Exonum combines secret part of
//!   the node configuration with public configurations of every other node, producing a single
//!   configuration file with all the necessary node and network settings.
//! 4. Use `run` command and provide it with final node configuration file produced at the previous
//!   step. If the secret keys are protected with passwords, the user need to enter the password.
//!   Running node will automatically connect to other nodes in the network using IP addresses from
//!   public parts of the node configurations.
//!
//! ## Additional Commands
//!
//! `exonum-cli` also supports additional CLI commands for performing maintenance actions by node
//! administrators and easier debugging.
//!
//! * `run-dev` command automatically generates network configuration with a single node and runs
//! it. This command can be useful for fast testing of the services during development process.
//! * `maintenance` command contains only `clear-cache` functionality at the moment. It allows to
//! clear node's consensus messages cache to fix rare node out-of-sync issues.
//!
//! ## How to Extend Parameters
//!
//! `exonum-cli` allows to extend the list of the parameters for any command and even add new CLI
//! commands with arbitrary behavior. To do so, you need to implement a structure with a list of
//! additional parameters and use `flatten` macro attribute of [`serde`][serde] and
//! [`structopt`][structopt] libraries.
//!
//! ```ignore
//! #[derive(Serialize, Deserialize, StructOpt)]
//! struct MyRunCommand {
//!     #[serde(flatten)]
//!     #[structopt(flatten)]
//!     default: Run
//!     /// My awesome parameter
//!     secret_number: i32
//! }
//! ```
//!
//! You can also create own list of commands by implementing an enum with a similar principle:
//!
//! ```ignore
//! #[derive(StructOpt)]
//! enum MyCommands {
//!     #[structopt(name = "run")
//!     DefaultRun(Run),
//!     #[structopt(name = "my-run")
//!     MyAwesomeRun(MyRunCommand),
//! }
//! ```
//!
//! While implementing custom behavior for your commands, you may use
//! [`StandardResult`](./command/enum.StandardResult.html) enum for
//! accessing node configuration files created and filled by the standard Exonum commands.
//!
//! [serde]: https://crates.io/crates/serde
//! [structopt]: https://crates.io/crates/structopt

pub extern crate structopt;

use exonum::{
    blockchain::InstanceCollection,
    exonum_merkledb::{Database, RocksDB},
    node::Node,
    runtime::rust::ServiceFactory,
};

use std::sync::Arc;

use crate::command::{Command, ExonumCommand, StandardResult};

pub mod command;
pub mod config;
pub mod io;
pub mod password;

/// Rust-specific node builder used for constructing a node with a list
/// of provided services.
#[derive(Debug, Default)]
pub struct NodeBuilder {
    services: Vec<Box<dyn ServiceFactory>>,
}

impl NodeBuilder {
    /// Creates new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds new Rust service to the list of available services.
    pub fn with_service(mut self, service: impl Into<Box<dyn ServiceFactory>>) -> Self {
        self.services.push(service.into());
        self
    }

    /// Configures the node using parameters provided by user from stdin and then runs it.
    ///
    /// Only Rust runtime is enabled.
    pub fn run(self) -> Result<(), failure::Error> {
        let command = Command::from_args();
        if let StandardResult::Run(run_config) = command.execute()? {
            let db_options = &run_config.node_config.database;
            let database: Arc<dyn Database> =
                Arc::new(RocksDB::open(run_config.db_path, db_options)?);

            let node_config_path = run_config.node_config_path.to_string_lossy().to_string();
            let node = Node::new(
                database,
                self.services.into_iter().map(InstanceCollection::new),
                run_config.node_config,
                Some(node_config_path),
            );

            node.run()
        } else {
            Ok(())
        }
    }
}