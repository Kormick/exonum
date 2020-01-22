// Copyright 2020 The Exonum Team
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

//! Helpers for a Dispatcher.

use exonum_merkledb::Fork;

use crate::runtime::dispatcher::Schema;

/// Removes local migration result for specified service.
pub fn remove_local_migration_result(fork: &Fork, service_name: &str) {
    Schema::new(fork)
        .local_migration_results()
        .remove(service_name);
}
