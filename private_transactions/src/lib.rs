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

//! Private transactions module.
/// Export encryptor module.
pub mod encryptor;
/// Export the private_transactions module.
pub mod private_transactions;
mod messages;
mod error;

extern crate ethcore;
extern crate ethcore_bytes as bytes;
extern crate ethcore_transaction as transaction;
extern crate ethcore_miner;
extern crate ethcrypto;
extern crate ethabi;
extern crate ethereum_types;
extern crate ethkey;
extern crate ethjson;
extern crate fetch;
extern crate futures;
extern crate keccak_hash as hash;
extern crate native_contracts;
extern crate parking_lot;
extern crate patricia_trie as trie;
extern crate rlp;
extern crate rustc_hex;
#[macro_use]
extern crate log;

#[cfg(test)]
extern crate rand;
#[cfg(test)]
extern crate ethcore_logger;

pub use self::encryptor::{Encryptor, SecretStoreEncryptor, DummyEncryptor, EncryptorConfig};
pub use self::private_transactions::{PrivateTransactionDesc, VerificationStore, PrivateTransactionSigningDesc, SigningStore};
pub use self::messages::{PrivateTransaction, SignedPrivateTransaction};
pub use self::error::PrivateTransactionError;

use std::sync::{Arc, Weak};
use std::collections::HashMap;
use ethereum_types::{H128, H256, U256, Address};
use hash::keccak;
use rlp::*;
use parking_lot::{Mutex, RwLock};
use futures::Future;
use bytes::Bytes;
use ethkey::{Signature, recover, public_to_address};
use ethcore::executive::{Executive, TransactOptions};
use ethcore::executed::{Executed};
use transaction::{SignedTransaction, Transaction, Action, UnverifiedTransaction};
use ethcore::{contract_address as ethcore_contract_address};
use ethcore::client::{Client, BlockChainClient, ChainNotify, ChainMessageType, BlockId, MiningBlockChainClient, PrivateNotify};
use ethcore::account_provider::AccountProvider;
use ethcore::service::ClientIoMessage;
use ethcore::error::TransactionImportError;
use ethcore_miner::transaction_queue::{TransactionDetailsProvider as TransactionQueueDetailsProvider, AccountDetails};
use ethcore::miner::MinerService;
use native_contracts::Private as Contract;
use ethcore::trace::{Tracer, VMTracer};
use rustc_hex::FromHex;

// Source avaiable at https://github.com/paritytech/contracts/blob/master/contracts/PrivateContract.sol
const DEFAULT_STUB_CONTRACT: &'static str = "6060604052341561000f57600080fd5b6040516109ce3803806109ce83398101604052808051820191906020018051820191906020018051820191905050826000908051906020019061005392919061008a565b50816002908051906020019061006a929190610114565b508060019080519060200190610081929190610114565b505050506101fc565b828054828255906000526020600020908101928215610103579160200282015b828111156101025782518260006101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff160217905550916020019190600101906100aa565b5b5090506101109190610194565b5090565b828054600181600116156101000203166002900490600052602060002090601f016020900481019282601f1061015557805160ff1916838001178555610183565b82800160010185558215610183579182015b82811115610182578251825591602001919060010190610167565b5b50905061019091906101d7565b5090565b6101d491905b808211156101d057600081816101000a81549073ffffffffffffffffffffffffffffffffffffffff02191690555060010161019a565b5090565b90565b6101f991905b808211156101f55760008160009055506001016101dd565b5090565b90565b6107c38061020b6000396000f30060606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff16806317ac53a21461005e5780631865c57d1461017b578063b7ab4db514610209578063ea8796341461027357600080fd5b341561006957600080fd5b610179600480803590602001908201803590602001908080601f01602080910402602001604051908101604052809392919081815260200183838082843782019150505050505091908035906020019082018035906020019080806020026020016040519081016040528093929190818152602001838360200280828437820191505050505050919080359060200190820180359060200190808060200260200160405190810160405280939291908181526020018383602002808284378201915050505050509190803590602001908201803590602001908080602002602001604051908101604052809392919081815260200183836020028082843782019150505050505091905050610301565b005b341561018657600080fd5b61018e6104e6565b6040518080602001828103825283818151815260200191508051906020019080838360005b838110156101ce5780820151818401526020810190506101b3565b50505050905090810190601f1680156101fb5780820380516001836020036101000a031916815260200191505b509250505060405180910390f35b341561021457600080fd5b61021c61058e565b6040518080602001828103825283818151815260200191508051906020019060200280838360005b8381101561025f578082015181840152602081019050610244565b505050509050019250505060405180910390f35b341561027e57600080fd5b610286610622565b6040518080602001828103825283818151815260200191508051906020019080838360005b838110156102c65780820151818401526020810190506102ab565b50505050905090810190601f1680156102f35780820380516001836020036101000a031916815260200191505b509250505060405180910390f35b600080856040518082805190602001908083835b60208310151561033a5780518252602082019150602081019050602083039250610315565b6001836020036101000a03801982511681845116808217855250505050505090500191505060405180910390209150600090505b6000805490508110156104c75760008181548110151561038a57fe5b906000526020600020900160009054906101000a900473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1660018387848151811015156103dd57fe5b9060200190602002015187858151811015156103f557fe5b90602001906020020151878681518110151561040d57fe5b90602001906020020151604051600081526020016040526000604051602001526040518085600019166000191681526020018460ff1660ff16815260200183600019166000191681526020018260001916600019168152602001945050505050602060405160208103908084039060008661646e5a03f1151561048f57600080fd5b50506020604051035173ffffffffffffffffffffffffffffffffffffffff161415156104ba57600080fd5b808060010191505061036e565b85600190805190602001906104dd9291906106ca565b50505050505050565b6104ee61074a565b60018054600181600116156101000203166002900480601f0160208091040260200160405190810160405280929190818152602001828054600181600116156101000203166002900480156105845780601f1061055957610100808354040283529160200191610584565b820191906000526020600020905b81548152906001019060200180831161056757829003601f168201915b5050505050905090565b61059661075e565b600080548060200260200160405190810160405280929190818152602001828054801561061857602002820191906000526020600020905b8160009054906101000a900473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190600101908083116105ce575b5050505050905090565b61062a61074a565b60028054600181600116156101000203166002900480601f0160208091040260200160405190810160405280929190818152602001828054600181600116156101000203166002900480156106c05780601f10610695576101008083540402835291602001916106c0565b820191906000526020600020905b8154815290600101906020018083116106a357829003601f168201915b5050505050905090565b828054600181600116156101000203166002900490600052602060002090601f016020900481019282601f1061070b57805160ff1916838001178555610739565b82800160010185558215610739579182015b8281111561073857825182559160200191906001019061071d565b5b5090506107469190610772565b5090565b602060405190810160405280600081525090565b602060405190810160405280600081525090565b61079491905b80821115610790576000816000905550600101610778565b5090565b905600a165627a7a7230582012a0ab4be8ba61a3fc7601b05ab9c31c619ceccc4ff930f31cd28e140bcb4d340029";

/// Initialization vector length.
const INIT_VEC_LEN: usize = 16;

struct TransactionDetailsProvider<'a> {
	client: &'a MiningBlockChainClient,
}

impl<'a> TransactionDetailsProvider<'a> {
	pub fn new(client: &'a MiningBlockChainClient) -> Self {
		TransactionDetailsProvider {
			client: client,
		}
	}
}

impl<'a> TransactionQueueDetailsProvider for TransactionDetailsProvider<'a> {
	fn fetch_account(&self, address: &Address) -> AccountDetails {
		AccountDetails {
			nonce: self.client.latest_nonce(address),
			balance: self.client.latest_balance(address),
		}
	}

	fn estimate_gas_required(&self, tx: &SignedTransaction) -> U256 {
		tx.gas_required(&self.client.latest_schedule()).into()
	}

	fn is_service_transaction_acceptable(&self, _tx: &SignedTransaction) -> Result<bool, String> {
		Ok(false)
	}
}

/// Configurtion for private transaction provider
#[derive(Default, PartialEq, Debug, Clone)]
pub struct ProviderConfig {
	/// Accounts that can be used for validation
	pub validator_accounts: Vec<Address>,
	/// Account used for signing public transactions created from private transactions
	pub signer_account: Option<Address>,
	/// Passwords used to unlock accounts
	pub passwords: Vec<String>,
}

#[derive(Debug)]
/// Private transaction execution receipt.
pub struct Receipt {
	/// Private transaction hash.
	pub hash: H256,
	/// Created contract address if any.
	pub contract_address: Option<Address>,
	/// Execution status.
	pub status_code: u8,
}

/// Manager of private transactions
pub struct Provider {
	encryptor: Arc<Encryptor>,
	config: ProviderConfig,
	notify: RwLock<Vec<Weak<ChainNotify>>>,
	transactions_for_signing: Mutex<SigningStore>,
	transactions_for_verification: Mutex<VerificationStore>,
	client: Arc<Client>,
	accounts: Arc<AccountProvider>,
}

#[derive(Debug)]
struct PrivateExecutionResult<T, V> where T: Tracer, V: VMTracer {
	code: Option<Bytes>,
	state: Bytes,
	result: Executed<T::Output, V::Output>,
}

impl Provider where {
	/// Create a new provider.
	pub fn new(client: Arc<Client>, accounts: Arc<AccountProvider>, encryptor: Arc<Encryptor>, config: ProviderConfig) -> Result<Self, PrivateTransactionError> {
		Ok(Provider {
			config: config,
			notify: RwLock::new(Vec::new()),
			transactions_for_signing: Mutex::new(SigningStore::new()),
			transactions_for_verification: Mutex::new(VerificationStore::new()),
			client: client,
			accounts: accounts,
			encryptor: encryptor,
		})
	}

	/// Adds an actor to be notified on certain events
	pub fn add_notify(&self, target: Arc<ChainNotify>) {
		self.notify.write().push(Arc::downgrade(&target));
	}

	fn notify<F>(&self, f: F) where F: Fn(&ChainNotify) {
		for np in self.notify.read().iter() {
			if let Some(n) = np.upgrade() {
				f(&*n);
			}
		}
	}

	/// 1. Create private transaction from the signed transaction
	/// 2. Executes private transaction
	/// 3. Save it with state returned on prev step to the queue for signing
	/// 4. Broadcast corresponding message to the chain
	pub fn create_private_transaction(&self, signed_transaction: SignedTransaction) -> Result<Receipt, PrivateTransactionError> {
		trace!("Creating private transaction from regular transaction: {:?}", signed_transaction);
		if self.config.signer_account.is_none() {
			trace!("Signing account not set");
			return Err(PrivateTransactionError::SignerAccountNotSet.into());
		}
		let tx_hash = signed_transaction.hash();
		match signed_transaction.action {
			Action::Create => {
				return Err(PrivateTransactionError::BadTransactonType.into());
			}
			Action::Call(contract) => {
				let data = signed_transaction.rlp_bytes();
				let encrypted_transaction = self.encrypt(&contract, &Self::iv_from_transaction(&signed_transaction), &data)?;
				let private = PrivateTransaction {
					encrypted: encrypted_transaction,
					contract: contract,
				};
				let private_state = self.execute_private_transaction(BlockId::Latest, &signed_transaction)?;
				trace!("Private transaction created, encrypted transaction: {:?}, private state: {:?}", private, private_state);
				let contract_validators = self.get_validators(BlockId::Latest, &contract)?;
				trace!("Required validators: {:?}", contract_validators);
				let private_state_hash = keccak(&private_state);
				trace!("Hashed effective private state for sender: {:?}", private_state_hash);
				self.transactions_for_signing.lock().add_transaction(private.hash(), signed_transaction, contract_validators, private_state)?;
				self.broadcast_private_transaction(private.rlp_bytes().into_vec());
				Ok(Receipt {
					hash: tx_hash,
					contract_address: None,
					status_code: 0,
				})
			}
		}
	}

	/// Try to unlock account using stored passwords
	fn unlock_account(&self, account: &Address) -> Result<bool, PrivateTransactionError> {
		let passwords = self.config.passwords.clone();
		for password in passwords {
			if let Ok(()) = self.accounts.unlock_account_temporarily(account.clone(), password) {
				return Ok(true);
			}
		}
		Ok(false)
	}

	/// Extract signed transaction from private transaction
	fn extract_original_transaction(&self, private: PrivateTransaction, contract: &Address) -> Result<UnverifiedTransaction, PrivateTransactionError> {
		let encrypted_transaction = private.encrypted.clone();
		let transaction_bytes = self.decrypt(contract, &encrypted_transaction)?;
		let original_transaction: UnverifiedTransaction = UntrustedRlp::new(&transaction_bytes).as_val()?;
		Ok(original_transaction)
	}

	/// Process received private transaction
	pub fn import_private_transaction(&self, rlp: &[u8]) -> Result<(), PrivateTransactionError> {
		let validator_accounts = self.config.validator_accounts.clone();
		trace!("Private transaction received");
		if validator_accounts.is_empty() {
			self.broadcast_private_transaction(rlp.into());
			return Ok(());
		}
		let private_tx: PrivateTransaction = UntrustedRlp::new(rlp).as_val()?;
		let contract = private_tx.contract;
		let contract_validators = self.get_validators(BlockId::Latest, &contract)?;

		match contract_validators
			.iter()
			.filter(|&&address| validator_accounts
				.iter()
				.any(|&validator| validator == address))
			.next() {
			None => {
				// Not for verification, broadcast further to peers
				self.broadcast_private_transaction(rlp.into());
				return Ok(());
			},
			Some(&validation_account) => {
				trace!("Private transaction taken for verification");
				let original_tx = self.extract_original_transaction(private_tx.clone(), &contract)?;
				trace!("Validating transaction: {:?}", original_tx);
				let details_provider = TransactionDetailsProvider::new(&*self.client as &MiningBlockChainClient);
				let insertion_time = self.client.chain_info().best_block_number;
				// Verify with the first account available
				trace!("The following account will be used for verification: {:?}", validation_account);
				let add_res = self.transactions_for_verification.lock().add_transaction(original_tx, contract, validation_account, private_tx.hash(), &details_provider, insertion_time);
				match add_res {
					Ok(_) => {
						let channel = self.client.get_io_channel();
						let channel = channel.lock();
						channel.send(ClientIoMessage::NewPrivateTransaction)
							.map_err(|_| PrivateTransactionError::ClientIsMalformed.into())
					},
					Err(err) => Err(err),
				}
			}
		}
	}

	/// Private transaction for validation added into queue
	pub fn on_private_transaction_queued(&self) -> Result<(), PrivateTransactionError> {
		self.process_queue()
	}

	/// Retrieve and verify the first available private transaction for every sender
	fn process_queue(&self) -> Result<(), PrivateTransactionError> {
		let mut verification_queue = self.transactions_for_verification.lock();
		let ready_transactions = verification_queue.ready_transactions();
		let fetch_nonce = |a: &Address| self.client.latest_nonce(a);
		for transaction in ready_transactions {
			let transaction_hash = transaction.hash();
			match verification_queue.private_transaction_descriptor(&transaction_hash) {
				Ok(desc) => {
					match self.config.validator_accounts.iter().find(|&&account| account == desc.validator_account) {
						Some(account) => {
							let private_state = self.execute_private_transaction(BlockId::Latest, &transaction)?;
							let private_state_hash = keccak(&private_state);
							trace!("Hashed effective private state for validator: {:?}", private_state_hash);
							if let Ok(true) = self.unlock_account(&account) {
								let signed_state = self.accounts.sign(account.clone(), None, private_state_hash)?;
								let signed_private_transaction = SignedPrivateTransaction::new(desc.private_hash, signed_state, None);
								trace!("Sending signature for private transaction: {:?}", signed_private_transaction);
								self.broadcast_signed_private_transaction(signed_private_transaction.rlp_bytes().into_vec());
							} else {
								trace!("Cannot unlock account");
							}
						}
						None => trace!("Cannot find validator account in config"),
					}
				},
				Err(e) => trace!("Cannot retrieve descriptor for transaction with error {:?}", e),
			}
			verification_queue.remove_private_transaction(&transaction_hash, &fetch_nonce);
		}
		Ok(())
	}

	/// Add signed private transaction into the store
	/// Creates corresponding public transaction if last required singature collected and sends it to the chain
	pub fn import_signed_private_transaction(&self, rlp: &[u8]) -> Result<(), PrivateTransactionError> {
		let tx: SignedPrivateTransaction = UntrustedRlp::new(rlp).as_val()?;
		trace!("Signature for private transaction received: {:?}", tx);
		let private_hash = tx.private_transaction_hash();
		if self.transactions_for_signing.lock().get(&private_hash).is_none() {
			// Not our transaction, broadcast further to peers
			self.broadcast_signed_private_transaction(rlp.into());
			return Ok(());
		}

		let desc = self.transactions_for_signing.lock().get(&private_hash).expect("None was checked before; qed");
		let last = self.last_required_signature(&desc, tx.signature())?;

		if last {
			let mut signatures = desc.received_signatures.clone();
			signatures.push(tx.signature());
			let rsv: Vec<Signature> = signatures.into_iter().map(|sign| sign.into_electrum().into()).collect();
			//Create public transaction
			let public_tx = self.public_transaction(
				desc.state.clone(),
				&desc.original_transaction.clone(),
				&rsv,
				desc.original_transaction.nonce,
				desc.original_transaction.gas_price
			)?;
			trace!("Last required signature received, public transaction created: {:?}", public_tx);
			//Sign and add it to the queue
			let chain_id = desc.original_transaction.chain_id();
			let hash = public_tx.hash(chain_id);
			let signer_account = self.config.signer_account.ok_or_else(|| PrivateTransactionError::SignerAccountNotSet)?;
			if let Ok(true) = self.unlock_account(&signer_account) {
				let signature = self.accounts.sign(signer_account.clone(), None, hash)?;
				let signed = SignedTransaction::new(public_tx.with_signature(signature, chain_id))?;
				match self.client.miner().import_own_transaction(&*self.client as &MiningBlockChainClient, signed.into()) {
					Ok(_) => trace!("Public transaction added to queue"),
					Err(err) => trace!("Failed to add transaction to queue, error: {:?}", err),
				}
			} else {
				trace!("Cannot unlock account");
			}
			//Remove from store for signing
			match self.transactions_for_signing.lock().remove(&private_hash) {
				Ok(_) => {}
				Err(err) => trace!("Failed to remove transaction from signing store, error: {:?}", err),
			}
		} else {
			//Add signature to the store
			match self.transactions_for_signing.lock().add_signature(&private_hash, tx.signature()) {
				Ok(_) => trace!("Signature stored for private transaction"),
				Err(err) => trace!("Failed to add signature to signing store, error: {:?}", err),
			}
		}
		Ok(())
	}

	fn last_required_signature(&self, desc: &PrivateTransactionSigningDesc, sign: Signature) -> Result<bool, PrivateTransactionError>  {
		if desc.received_signatures.contains(&sign) {
			return Ok(false);
		}
		let state_hash = keccak(&desc.state);
		match recover(&sign, &state_hash) {
			Ok(public) => {
				let sender = public_to_address(&public);
				match desc.validators.contains(&sender) {
					true => {
						Ok(desc.received_signatures.len() + 1 == desc.validators.len())
					}
					false => {
						trace!("Sender's state doesn't correspond to validator's");
						Err(PrivateTransactionError::StateIncorrect.into())
					}
				}
			}
			Err(err) => {
				trace!("Sender's state doesn't correspond to validator's, error {:?}", err);
				Err(err.into())
			}
		}
	}

	/// Broadcast the private transaction message to the chain
	fn broadcast_private_transaction(&self, message: Bytes) {
		self.notify(|notify| notify.broadcast(ChainMessageType::PrivateTransaction, message.clone()));
	}

	/// Broadcast signed private transaction message to the chain
	fn broadcast_signed_private_transaction(&self, message: Bytes) {
		self.notify(|notify| notify.broadcast(ChainMessageType::SignedPrivateTransaction, message.clone()));
	}

	fn iv_from_transaction(transaction: &SignedTransaction) -> H128 {
		let nonce = keccak(&transaction.nonce.rlp_bytes());
		let (iv, _) = nonce.split_at(INIT_VEC_LEN);
		H128::from_slice(iv)
	}

	fn iv_from_address(contract_address: &Address) -> H128 {
		let address = keccak(&contract_address.rlp_bytes());
		let (iv, _) = address.split_at(INIT_VEC_LEN);
		H128::from_slice(iv)
	}

	fn encrypt(&self, contract_address: &Address, initialisation_vector: &H128, data: &[u8]) -> Result<Bytes, PrivateTransactionError> {
		trace!("Encrypt data using key(address): {:?}", contract_address);
		Ok(self.encryptor.encrypt(contract_address, self.accounts.clone(), initialisation_vector, data)?)
	}

	fn decrypt(&self, contract_address: &Address, data: &[u8]) -> Result<Bytes, PrivateTransactionError> {
		trace!("Decrypt data using key(address): {:?}", contract_address);
		Ok(self.encryptor.decrypt(contract_address, self.accounts.clone(), data)?)
	}

	fn get_decrypted_state(&self, address: &Address, block: BlockId) -> Result<Bytes, PrivateTransactionError> {
		let contract = Contract::new(*address);
		let state = contract.get_state(|addr, data| futures::done(self.client.call_contract(block, addr, data))).wait()
			.map_err(|e| PrivateTransactionError::Call(e))?;
		self.decrypt(address, &state)
	}

	fn get_decrypted_code(&self, address: &Address, block: BlockId) -> Result<Bytes, PrivateTransactionError> {
		let contract = Contract::new(*address);
		let code = contract.get_code(|addr, data| futures::done(self.client.call_contract(block, addr, data))).wait()
			.map_err(|e| PrivateTransactionError::Call(e))?;
		self.decrypt(address, &code)
	}

	fn snapshot_to_storage(raw: Bytes) -> HashMap<H256, H256> {
		let items = raw.len() / 64;
		(0..items).map(|i| {
			let offset = i * 64;
			let key = H256::from_slice(&raw[offset..(offset + 32)]);
			let value = H256::from_slice(&raw[(offset + 32)..(offset + 64)]);
			(key, value)
		}).collect()
	}

	fn snapshot_from_storage(storage: &HashMap<H256, H256>) -> Bytes {
		let mut raw = Vec::with_capacity(storage.len() * 64);
		for (key, value) in storage {
			raw.extend_from_slice(key);
			raw.extend_from_slice(value);
		};
		raw
	}

	fn execute_private<T, V>(&self, transaction: &SignedTransaction, options: TransactOptions<T, V>, block: BlockId) -> Result<PrivateExecutionResult<T, V>, PrivateTransactionError>
		where
			T: Tracer,
			V: VMTracer,
	{
		let mut env_info = self.client.env_info(block).ok_or(PrivateTransactionError::StatePruned)?;
		env_info.gas_limit = transaction.gas;

		let mut state = self.client.state_at(block).ok_or(PrivateTransactionError::StatePruned)?;
		// TODO: in case of BlockId::Latest these need to operate on the same state
		let contract_address = match &transaction.action {
			&Action::Call(ref contract_address) => {
				let contract_code = Arc::new(self.get_decrypted_code(contract_address, block)?);
				let contract_state = self.get_decrypted_state(contract_address, block)?;
				trace!("Patching contract at {:?}, code: {:?}, state: {:?}", contract_address, contract_code, contract_state);
				state.patch_account(contract_address, contract_code.clone(), Self::snapshot_to_storage(contract_state))?;
				Some(contract_address.clone())
			},
			&Action::Create => None,
		};

		let engine = self.client.engine();
		let contract_address = contract_address.or({
			let sender = transaction.sender();
			let nonce = state.nonce(&sender)?;
			let (new_address, _) = ethcore_contract_address(engine.create_address_scheme(env_info.number), &sender, &nonce, &transaction.data);
			Some(new_address)
		});
		let result = Executive::new(&mut state, &env_info, engine.machine()).transact_virtual(transaction, options)?;
		let (encrypted_code, encrypted_storage) = match contract_address {
			Some(address) => {
				let (code, storage) = state.into_account(&address)?;
				let enc_code = match code {
					Some(c) => Some(self.encrypt(&address, &Self::iv_from_address(&address), &c)?),
					None => None,
				};
				(enc_code, self.encrypt(&address, &Self::iv_from_transaction(transaction), &Self::snapshot_from_storage(&storage))?)
			},
			None => return Err(PrivateTransactionError::ContractDoesNotExist.into())
		};
		trace!("Private contract executed. code: {:?}, state: {:?}, result: {:?}", encrypted_code, encrypted_storage, result.output);
		Ok(PrivateExecutionResult {
			code: encrypted_code,
			state: encrypted_storage,
			result: result,
		})
	}

	fn generate_constructor(validators: &[Address], code: Bytes, storage: Bytes) -> Bytes {
		let constructor_code = DEFAULT_STUB_CONTRACT.from_hex().expect("Default contract code is valid");
		let constructor = ethabi::Constructor { inputs: vec![
			ethabi::Param { name: "v".into(), kind: ethabi::ParamType::Array(Box::new(ethabi::ParamType::Address)) },
			ethabi::Param { name: "c".into(), kind: ethabi::ParamType::Bytes },
			ethabi::Param { name: "s".into(), kind: ethabi::ParamType::Bytes },
		]};

		let tokens = [
			ethabi::Token::Array(validators.iter().map(|a| ethabi::Token::Address(a.clone().0)).collect()),
			ethabi::Token::Bytes(code),
			ethabi::Token::Bytes(storage),
		];

		constructor.encode_input(constructor_code, &tokens).expect("Input is always valid")
	}

	fn generate_set_state_call(signatures: &[Signature], storage: Bytes) -> Bytes {
		let function = ethabi::Function { name: "setState".into(), constant:false, outputs: vec![], inputs: vec![
			ethabi::Param { name: "ns".into(), kind: ethabi::ParamType::Bytes },
			ethabi::Param { name: "v".into(), kind: ethabi::ParamType::Array(Box::new(ethabi::ParamType::Uint(8))) },
			ethabi::Param { name: "r".into(), kind: ethabi::ParamType::Array(Box::new(ethabi::ParamType::FixedBytes(32))) },
			ethabi::Param { name: "s".into(), kind: ethabi::ParamType::Array(Box::new(ethabi::ParamType::FixedBytes(32))) },
		]};

		let tokens = [
			ethabi::Token::Bytes(storage),
			ethabi::Token::Array(signatures.iter().map(|s| {
				let mut v: [u8; 32] = [0; 32];
				v[31] = s.v();
				ethabi::Token::Uint(v)
			}).collect()),
			ethabi::Token::Array(signatures.iter().map(|s| ethabi::Token::FixedBytes(s.r().to_vec())).collect()),
			ethabi::Token::Array(signatures.iter().map(|s| ethabi::Token::FixedBytes(s.s().to_vec())).collect()),
		];

		function.encode_input(&tokens).expect("Input is always valid")
	}

	/// Returns the key from the key server associated with the contract
	pub fn contract_key_id(&self, contract_address: &Address) -> Result<H256, PrivateTransactionError> {
		//current solution uses contract address extended with 0 as id
		let contract_address_extended: H256 = contract_address.into();

		Ok(H256::from_slice(&contract_address_extended))
	}

	/// Create encrypted public contract deployment transaction.
	pub fn public_creation_transaction(&self, block: BlockId, source: &SignedTransaction, validators: &[Address], gas_price: U256) -> Result<Transaction, PrivateTransactionError> {
		if let &Action::Call(_) = &source.action {
			return Err(PrivateTransactionError::BadTransactonType.into());
		}
		let sender = source.sender();
		let state = self.client.state_at(block).ok_or(PrivateTransactionError::StatePruned)?;
		let nonce = state.nonce(&sender)?;
		let executed = self.execute_private(source, TransactOptions::with_no_tracing(), block)?;
		let gas: u64 = 650000 +
			validators.len() as u64 * 30000 +
			executed.code.as_ref().map_or(0, |c| c.len() as u64) * 8000 +
			executed.state.len() as u64 * 8000;
		Ok(Transaction {
			nonce: nonce,
			action: Action::Create,
			gas: gas.into(),
			gas_price: gas_price,
			value: source.value,
			data: Self::generate_constructor(validators, executed.code.unwrap_or_default(), executed.state)
		})
	}

	/// Create encrypted public contract deployment transaction. Returns updated encrypted state.
	fn execute_private_transaction(&self, block: BlockId, source: &SignedTransaction) -> Result<Bytes, PrivateTransactionError> {
		if let &Action::Create = &source.action {
			return Err(PrivateTransactionError::BadTransactonType.into());
		}
		let result = self.execute_private(source, TransactOptions::with_no_tracing(), block)?;
		Ok(result.state)
	}

	/// Create encrypted public contract deployment transaction.
	pub fn public_transaction(&self, state: Bytes, source: &SignedTransaction, signatures: &[Signature], nonce: U256, gas_price: U256) -> Result<Transaction, PrivateTransactionError> {
		let gas: u64 = 650000 + state.len() as u64 * 8000 + signatures.len() as u64 * 50000;
		Ok(Transaction {
			nonce: nonce,
			action: source.action.clone(),
			gas: gas.into(),
			gas_price: gas_price,
			value: 0.into(),
			data: Self::generate_set_state_call(signatures, state)
		})
	}

	/// Call into private contract.
	pub fn private_call(&self, block: BlockId, transaction: &SignedTransaction) -> Result<Executed, PrivateTransactionError> {
		let result = self.execute_private(transaction, TransactOptions::with_no_tracing(), block)?;
		Ok(result.result)
	}

	/// Returns private validators for a contract.
	fn get_validators(&self, block: BlockId, contract: &Address) -> Result<Vec<Address>, PrivateTransactionError> {
		let contract = Contract::new(*contract);
		Ok(contract.get_validators(|addr, data| futures::done(self.client.call_contract(block, addr, data))).wait()
			.map_err(|e| PrivateTransactionError::Call(e))?)
	}
}

impl ChainNotify for Provider {
	fn new_blocks(&self, imported: Vec<H256>, _invalid: Vec<H256>, _enacted: Vec<H256>, _retracted: Vec<H256>, _sealed: Vec<H256>, _proposed: Vec<Bytes>, _duration: u64) {
		if !imported.is_empty() {
			trace!("New blocks imported, try to prune the queue");
			if let Err(err) = self.process_queue() {
				trace!("Cannot prune private transactions queue. error: {:?}", err);
			}
		}
	}
}

impl PrivateNotify for Provider {
	fn private_transaction_queued(&self) -> Result<(), TransactionImportError> {
		self.on_private_transaction_queued().map_err(|err| TransactionImportError::Other(format!("{:?}", err)))
	}
}

#[cfg(test)]
mod test {
	use std::sync::Arc;
	use rustc_hex::FromHex;
	use ethcore::client::BlockId;
	use hash::keccak;
	use ethkey::{Secret, KeyPair, Signature};
	use transaction::{Transaction, Action};
	use ethcore::executive::{contract_address};
	use ethcore::CreateContractAddress;
	use ethcore::client::BlockChainClient;
	use super::encryptor::DummyEncryptor;
	use super::{Provider, ProviderConfig};
	use ethcore::account_provider::AccountProvider;
	use ethcore::test_helpers::{generate_dummy_client, push_block_with_transactions};

	/// Contract code:
	#[test]
	fn private_contract() {
		// This uses a simple private contract: contract Test1 { bytes32 public x; function setX(bytes32 _x) { x = _x; } }
		::ethcore_logger::init_log();
		let client = generate_dummy_client(0);
		let chain_id = client.signing_chain_id();
		let key1 = KeyPair::from_secret(Secret::from("0000000000000000000000000000000000000000000000000000000000000011")).unwrap();
		let _key2 = KeyPair::from_secret(Secret::from("0000000000000000000000000000000000000000000000000000000000000012")).unwrap();
		let key3 = KeyPair::from_secret(Secret::from("0000000000000000000000000000000000000000000000000000000000000013")).unwrap();
		let key4 = KeyPair::from_secret(Secret::from("0000000000000000000000000000000000000000000000000000000000000014")).unwrap();
		let ap = Arc::new(AccountProvider::transient_provider());
		ap.insert_account(key1.secret().clone(), "").unwrap();
		ap.insert_account(key3.secret().clone(), "").unwrap();
		ap.insert_account(key4.secret().clone(), "").unwrap();

		let config = ProviderConfig{
			validator_accounts: vec![key3.address(), key4.address()],
			signer_account: None,
			passwords: vec!["".into()],
		};
		let pm = Arc::new(Provider::new(client.clone(), ap.clone(), Arc::new(DummyEncryptor::default()), config).unwrap());
		client.set_private_notify(pm.clone());

		let (address, _) = contract_address(CreateContractAddress::FromSenderAndNonce, &key1.address(), &0.into(), &[]);

		trace!("Creating private contract");
		let private_contract_test = "6060604052341561000f57600080fd5b60d88061001d6000396000f30060606040526000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff1680630c55699c146046578063bc64b76d14607457600080fd5b3415605057600080fd5b60566098565b60405180826000191660001916815260200191505060405180910390f35b3415607e57600080fd5b6096600480803560001916906020019091905050609e565b005b60005481565b8060008160001916905550505600a165627a7a723058206acbdf4b15ca4c2d43e1b1879b830451a34f1e9d02ff1f2f394d8d857e79d2080029".from_hex().unwrap();
		let mut private_create_tx = Transaction::default();
		private_create_tx.action = Action::Create;
		private_create_tx.data = private_contract_test;
		private_create_tx.gas = 200000.into();
		let private_create_tx_signed = private_create_tx.sign(&key1.secret(), None);
		let validators = vec![key3.address(), key4.address()];
		let public_tx = pm.public_creation_transaction(BlockId::Pending, &private_create_tx_signed, &validators, 0.into()).unwrap();
		let public_tx = public_tx.sign(&key1.secret(), chain_id);
		trace!("Transaction created. Pushing block");
		push_block_with_transactions(&client, &[public_tx]);

		trace!("Modifying private state");
		let mut private_tx = Transaction::default();
		private_tx.action = Action::Call(address.clone());
		private_tx.data = "bc64b76d2a00000000000000000000000000000000000000000000000000000000000000".from_hex().unwrap(); //setX(42)
		private_tx.gas = 120000.into();
		private_tx.nonce = 1.into();
		let private_tx = private_tx.sign(&key1.secret(), None);
		let private_state = pm.execute_private_transaction(BlockId::Latest, &private_tx).unwrap();
		let private_state_hash = keccak(&private_state);
		let signatures: Vec<_> = [&key3, &key4].iter().map(|k|
			Signature::from(::ethkey::sign(&k.secret(), &private_state_hash).unwrap().into_electrum())).collect();
		let public_tx = pm.public_transaction(private_state, &private_tx, &signatures, 1.into(), 0.into()).unwrap();
		let public_tx = public_tx.sign(&key1.secret(), chain_id);
		push_block_with_transactions(&client, &[public_tx]);

		trace!("Querying private state");
		let mut query_tx = Transaction::default();
		query_tx.action = Action::Call(address.clone());
		query_tx.data = "0c55699c".from_hex().unwrap();  // getX
		query_tx.gas = 50000.into();
		query_tx.nonce = 2.into();
		let query_tx = query_tx.sign(&key1.secret(), chain_id);
		let result = pm.private_call(BlockId::Latest, &query_tx).unwrap();
		assert_eq!(&result.output[..], &("2a00000000000000000000000000000000000000000000000000000000000000".from_hex().unwrap()[..]));
		assert_eq!(pm.get_validators(BlockId::Latest, &address).unwrap(), validators);

		// Now try modification with just one signature
		trace!("Modifying private state");
		let mut private_tx = Transaction::default();
		private_tx.action = Action::Call(address.clone());
		private_tx.data = "bc64b76d2b00000000000000000000000000000000000000000000000000000000000000".from_hex().unwrap(); //setX(42)
		private_tx.gas = 120000.into();
		private_tx.nonce = 2.into();
		let private_tx = private_tx.sign(&key1.secret(), None);
		let private_state = pm.execute_private_transaction(BlockId::Latest, &private_tx).unwrap();
		let private_state_hash = keccak(&private_state);
		let signatures: Vec<_> = [&key4].iter().map(|k|
			Signature::from(::ethkey::sign(&k.secret(), &private_state_hash).unwrap().into_electrum())).collect();
		let public_tx = pm.public_transaction(private_state, &private_tx, &signatures, 2.into(), 0.into()).unwrap();
		let public_tx = public_tx.sign(&key1.secret(), chain_id);
		push_block_with_transactions(&client, &[public_tx]);

		trace!("Querying private state");
		let mut query_tx = Transaction::default();
		query_tx.action = Action::Call(address.clone());
		query_tx.data = "0c55699c".from_hex().unwrap();  // getX
		query_tx.gas = 50000.into();
		query_tx.nonce = 3.into();
		let query_tx = query_tx.sign(&key1.secret(), chain_id);
		let result = pm.private_call(BlockId::Latest, &query_tx).unwrap();
		assert_eq!(&result.output[..], &("2a00000000000000000000000000000000000000000000000000000000000000".from_hex().unwrap()[..]));
	}
}

