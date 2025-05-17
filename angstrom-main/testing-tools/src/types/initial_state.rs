use alloy::providers::PendingTransaction;
use alloy_primitives::{
    Address, TxHash, U256,
    aliases::{I24, U24}
};
use angstrom_types::{
    contract_bindings::{angstrom::Angstrom::PoolKey, mintable_mock_erc_20::MintableMockERC20},
    matching::SqrtPriceX96
};

use crate::{
    contracts::{anvil::SafeDeployPending, environment::TestAnvilEnvironment},
    providers::WalletProvider,
    types::{HACKED_TOKEN_BALANCE, traits::WithWalletProvider}
};

pub struct PendingDeployedPools {
    pending_txs: Vec<PendingTransaction>,
    pool_keys:   Vec<PoolKey>
}

impl Default for PendingDeployedPools {
    fn default() -> Self {
        Self::new()
    }
}

impl PendingDeployedPools {
    pub fn new() -> Self {
        Self { pending_txs: Vec::new(), pool_keys: Vec::new() }
    }

    pub fn add_pending_tx(&mut self, tx: PendingTransaction) {
        self.pending_txs.push(tx)
    }

    pub fn add_pool_key(&mut self, pool_key: PoolKey) {
        self.pool_keys.push(pool_key)
    }

    pub fn pool_keys(&self) -> &[PoolKey] {
        &self.pool_keys
    }

    pub async fn finalize_pending_txs(&mut self) -> eyre::Result<(Vec<PoolKey>, Vec<TxHash>)> {
        let keys = std::mem::take(&mut self.pool_keys);

        let tx_hashes = futures::future::join_all(std::mem::take(&mut self.pending_txs))
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok((keys, tx_hashes))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PartialConfigPoolKey {
    // currency0:         Address,
    // currency1:         Address,
    pub fee:               u64,
    pub tick_spacing:      i32,
    pub initial_liquidity: u128,
    pub sqrt_price:        SqrtPriceX96
}

impl PartialConfigPoolKey {
    pub fn new(
        // currency0: Address,
        // currency1: Address,
        fee: u64,
        tick_spacing: i32,
        initial_liquidity: u128,
        sqrt_price: SqrtPriceX96
    ) -> Self {
        Self { fee, tick_spacing, initial_liquidity, sqrt_price }
    }

    pub fn make_pool_key(
        &self,
        angstrom_address_hook: Address,
        cur0: Address,
        cur1: Address
    ) -> PoolKey {
        PoolKey {
            currency0:   cur0,
            currency1:   cur1,
            fee:         U24::from(self.fee),
            tickSpacing: I24::unchecked_from(self.tick_spacing),
            hooks:       angstrom_address_hook
        }
    }

    pub fn initial_liquidity(&self) -> u128 {
        self.initial_liquidity
    }

    pub fn sqrt_price(&self) -> SqrtPriceX96 {
        self.sqrt_price
    }
}

#[derive(Debug, Clone)]
pub struct Erc20ToDeploy {
    pub name:            String,
    pub symbol:          String,
    pub overwrite_token: Option<Address>
}

impl Erc20ToDeploy {
    pub fn new(name: &str, symbol: &str, overwrite_token: Option<Address>) -> Self {
        Self { name: name.to_string(), symbol: symbol.to_string(), overwrite_token }
    }

    pub async fn deploy_token(
        &self,
        provider: &WalletProvider,
        nonce: &mut u64,
        pending_state: &mut PendingDeployedPools
    ) -> eyre::Result<Address> {
        let (pending_tx, token_address) =
            MintableMockERC20::deploy_builder(provider.provider_ref())
                .deploy_pending_creation(*nonce, provider.controller())
                .await?;

        pending_state.add_pending_tx(pending_tx);
        *nonce += 1;

        Ok(token_address)
    }

    pub async fn set_token_meta(
        &self,
        provider: &WalletProvider,
        token_address: Address,
        nonce: &mut u64,
        pending_state: &mut PendingDeployedPools
    ) -> eyre::Result<()> {
        let token_instance = MintableMockERC20::new(token_address, provider.rpc_provider());
        let pending_tx = token_instance
            .setMeta(self.name.clone(), self.symbol.clone())
            .nonce(*nonce)
            .deploy_pending()
            .await?;

        pending_state.add_pending_tx(pending_tx);
        *nonce += 1;

        Ok(())
    }

    pub async fn set_address_overrides(
        &self,
        provider: &WalletProvider,
        token_address: &mut Address,
        nonce: &mut u64,
        pending_state: &mut PendingDeployedPools,
        addresses_with_hacked_balance: Option<&[Address]>
    ) -> eyre::Result<()> {
        if let Some(overwrite_addr) = self.overwrite_token {
            provider
                .override_address(token_address, overwrite_addr)
                .await?;
        }

        tracing::debug!(
            "deployed token '{}' ('{}') at '{token_address:?}'",
            self.name,
            self.symbol
        );

        if let Some(addresses) = addresses_with_hacked_balance {
            let token_instance = MintableMockERC20::new(*token_address, provider.rpc_provider());
            for user_addr in addresses {
                let pending_tx = token_instance
                    .mint(*user_addr, U256::from(HACKED_TOKEN_BALANCE))
                    .nonce(*nonce)
                    .deploy_pending()
                    .await?;
                pending_state.add_pending_tx(pending_tx);

                *nonce += 1;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct InitialStateConfig {
    pub addresses_with_tokens: Vec<Address>,
    pub tokens_to_deploy:      Vec<Erc20ToDeploy>,
    pub pool_keys:             Vec<PartialConfigPoolKey>
}

#[derive(Debug, Clone, Copy)]
pub struct DeployedAddresses {
    pub angstrom_address:         Address,
    pub pool_gate_address:        Address,
    pub controller_v1_address:    Address,
    pub position_fetcher_address: Address,
    pub pool_manager_address:     Address,
    pub position_manager_address: Address
}
