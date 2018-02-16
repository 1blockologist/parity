// Copyright 2015-2017 Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

#![warn(missing_docs)]

//! Miner module
//! Keeps track of transactions and currently sealed pending block.

mod blockchain_client;
mod miner;

pub mod stratum;

pub use self::miner::{Miner, MinerOptions, PendingSet, AuthoringParams};

use std::sync::Arc;
use std::collections::BTreeMap;

use bytes::Bytes;
use ethereum_types::{H256, U256, Address};
use ethcore_miner::pool::VerifiedTransaction;

use block::ClosedBlock;
use client::{MiningBlockChainClient};
use error::{Error};
use header::BlockNumber;
use receipt::{RichReceipt, Receipt};
use transaction::{self, UnverifiedTransaction, PendingTransaction, SignedTransaction};

/// Miner client API
pub trait MinerService : Send + Sync {

	// Pending block

	/// Get the sealing work package and if `Some`, apply some transform.
	fn map_pending_block<F, T>(&self, chain: &MiningBlockChainClient, f: F) -> Option<T>
		where F: FnOnce(&ClosedBlock) -> T, Self: Sized;

	/// Get a list of all pending receipts from pending block.
	fn pending_receipts(&self, best_block: BlockNumber) -> Option<BTreeMap<H256, Receipt>>;

	/// Get a particular receipt from pending block.
	fn pending_receipt(&self, best_block: BlockNumber, hash: &H256) -> Option<RichReceipt>;

	/// Get all transactions in pending block or `None` if not sealing.
	fn pending_transactions(&self, best_block: BlockNumber) -> Option<Vec<SignedTransaction>>;

	// Block authoring / sealing

	/// Get current authoring parameters.
	fn authoring_params(&self) -> AuthoringParams;

	/// Set the lower and upper bound of gas limit we wish to target when sealing a new block.
	fn set_gas_range_target(&self, gas_range_target: (U256, U256));

	/// Set the extra_data that we will seal blocks with.
	fn set_extra_data(&self, extra_data: Bytes);

	/// Set info necessary to sign consensus messages and block authoring.
	///
	/// On PoW password is optional.
	fn set_author(&self, address: Address, password: Option<String>) -> Result<(), ::account_provider::SignError>;


	/// Is it currently sealing?
	fn is_currently_sealing(&self) -> bool;

	/// PoW chain - can produce work package
	fn can_produce_work_package(&self) -> bool;

	/// Submit `seal` as a valid solution for the header of `pow_hash`.
	/// Will check the seal, but not actually insert the block into the chain.
	fn submit_seal(&self, chain: &MiningBlockChainClient, pow_hash: H256, seal: Vec<Bytes>) -> Result<(), Error>;

	/// New chain head event. Restart mining operation.
	fn update_sealing(&self, chain: &MiningBlockChainClient);

	/// Called when blocks are imported to chain, updates transactions queue.
	fn chain_new_blocks(&self, chain: &MiningBlockChainClient, imported: &[H256], invalid: &[H256], enacted: &[H256], retracted: &[H256]);

	// Transactions and Pool

	/// Imports transactions to transaction queue.
	fn import_external_transactions(&self, chain: &MiningBlockChainClient, transactions: Vec<UnverifiedTransaction>) ->
		Vec<Result<(), transaction::Error>>;

	/// Imports own (node owner) transaction to queue.
	fn import_own_transaction(&self, chain: &MiningBlockChainClient, transaction: PendingTransaction) ->
		Result<(), transaction::Error>;

	/// Removes transaction from the queue.
	/// NOTE: The transaction is not removed from pending block if mining.
	// fn remove_pending_transaction(&self, chain: &MiningBlockChainClient, hash: &H256) -> Option<PendingTransaction>;

	/// Query pending transaction given it's hash.
	///
	/// Depending on the settings may look in transaction pool or only in pending block.
	fn transaction(&self, best_block: BlockNumber, hash: &H256) -> Option<PendingTransaction>;

	/// Get a list of all ready transactions.
	///
	/// Depending on the settings may look in transaction pool or only in pending block.
	fn ready_transactions(&self, chain: &MiningBlockChainClient) -> Vec<Arc<VerifiedTransaction>>;

	/// Get a list of all transactions in the pool (some of them might not be ready for inclusion yet).
	fn future_transactions(&self) -> Vec<Arc<VerifiedTransaction>>;

	/// Get a list of local transactions with statuses.
	// fn local_transactions(&self) -> BTreeMap<H256, LocalTransactionStatus>;

	/// Returns highest transaction nonce for given address.
	fn last_nonce(&self, address: &Address) -> Option<U256>;


	/// Suggested gas price.
	fn sensible_gas_price(&self) -> U256;

	/// Suggested gas limit.
	fn sensible_gas_limit(&self) -> U256;
}
