mod call;
mod genesis;
mod hooks;
#[cfg(feature = "native")]
mod query;

pub use call::CallMessage;
#[cfg(feature = "native")]
pub use query::{SequencerAddressResponse, SequencerRegistryRpcImpl, SequencerRegistryRpcServer};
use sov_modules_api::{CallResponse, Error, Spec};
use sov_modules_macros::ModuleInfo;
use sov_state::{StateMap, StateValue, WorkingSet};

/// Initial configuration for the sov_sequencer_registry module.
/// TODO: Should we allow multiple sequencers in genesis?
pub struct SequencerConfig<C: sov_modules_api::Context> {
    pub seq_rollup_address: C::Address,
    // TODO: Replace with DA address generic, when AddressTrait is split
    pub seq_da_address: Vec<u8>,
    pub coins_to_lock: sov_bank::Coins<C>,
    // TODO: Replace with DA address generic, when AddressTrait is split
    pub preferred_sequencer: Option<Vec<u8>>,
}

#[cfg_attr(feature = "native", derive(sov_modules_macros::ModuleCallJsonSchema))]
#[derive(ModuleInfo)]
pub struct SequencerRegistry<C: sov_modules_api::Context> {
    /// The address of the sov_sequencer_registry module
    /// Note: this is address is generated by the module framework and the corresponding private key is unknown.
    #[address]
    pub(crate) address: C::Address,

    /// Reference to the Bank module.
    #[module]
    pub(crate) bank: sov_bank::Bank<C>,

    /// Only batches from sequencers from this list are going to be processed
    ///
    #[state]
    pub(crate) allowed_sequencers: StateMap<Vec<u8>, C::Address>,

    /// Optional preferred sequencer
    /// If set, batches from this sequencer will be processed first in block,
    /// So this sequencer can guarantee soft confirmation time for transactions
    #[state]
    pub(crate) preferred_sequencer: StateValue<Vec<u8>>,

    /// Coin's that will be slashed if the sequencer is malicious.
    /// The coins will be transferred from `self.seq_rollup_address` to `self.address`
    /// and locked forever, until sequencer decides to exit
    /// Only sequencers in `allowed_sequencers` list are allowed to exit.
    #[state]
    pub(crate) coins_to_lock: StateValue<sov_bank::Coins<C>>,
}

/// Result of applying blob, from sequencer point of view.
pub enum SequencerOutcome {
    Completed,
    Slashed { sequencer: Vec<u8> },
}

impl<C: sov_modules_api::Context> sov_modules_api::Module for SequencerRegistry<C> {
    type Context = C;

    type Config = SequencerConfig<C>;

    type CallMessage = call::CallMessage;

    fn genesis(
        &self,
        config: &Self::Config,
        working_set: &mut WorkingSet<C::Storage>,
    ) -> Result<(), Error> {
        Ok(self.init_module(config, working_set)?)
    }

    fn call(
        &self,
        message: Self::CallMessage,
        context: &Self::Context,
        working_set: &mut WorkingSet<<Self::Context as Spec>::Storage>,
    ) -> Result<CallResponse, Error> {
        Ok(match message {
            call::CallMessage::Register { da_address } => {
                self.register(da_address, context, working_set)?
            }
            call::CallMessage::Exit { da_address } => {
                self.exit(da_address, context, working_set)?
            }
        })
    }
}

impl<C: sov_modules_api::Context> SequencerRegistry<C> {
    pub fn get_coins_to_lock(
        &self,
        working_set: &mut WorkingSet<C::Storage>,
    ) -> Option<sov_bank::Coins<C>> {
        self.coins_to_lock.get(working_set)
    }

    pub(crate) fn register_sequencer(
        &self,
        da_address: Vec<u8>,
        rollup_address: &C::Address,
        working_set: &mut WorkingSet<C::Storage>,
    ) -> anyhow::Result<()> {
        if self
            .allowed_sequencers
            .get(&da_address, working_set)
            .is_some()
        {
            anyhow::bail!("sequencer {} already registered", rollup_address)
        }
        let locker = &self.address;
        let coins = self.coins_to_lock.get_or_err(working_set)?;
        self.bank
            .transfer_from(rollup_address, locker, coins, working_set)?;

        self.allowed_sequencers
            .set(&da_address, rollup_address, working_set);

        Ok(())
    }

    /// Return preferred sequencer if it was set
    /// TODO: Replace with DA address generic, when AddressTrait is split
    pub fn get_preferred_sequencer(
        &self,
        working_set: &mut WorkingSet<C::Storage>,
    ) -> Option<Vec<u8>> {
        self.preferred_sequencer.get(working_set)
    }
}
