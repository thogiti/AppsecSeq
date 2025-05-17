use std::{pin::pin, sync::OnceLock, time::Duration};

use alloy::{
    contract::CallBuilder,
    primitives::{Address, U256, address},
    providers::{
        WalletProvider,
        ext::{AnvilApi, DebugApi}
    },
    rpc::types::trace::geth::{
        GethDebugBuiltInTracerType, GethDebugTracerType, GethDebugTracingOptions,
        GethDefaultTracingOptions
    }
};
use alloy_primitives::TxHash;
use angstrom_types::sol_bindings::testnet::{MockERC20, PoolManagerDeployer, TestnetHub};
use anvil::WalletProviderRpc;
use eyre::eyre;
use futures::Future;

pub mod anvil;
pub mod deploy;
pub mod environment;

/// This trait is used to provide safe run and potentially debug capabilities
/// for our local contract runs.
pub trait DebugTransaction {
    #[allow(async_fn_in_trait)] // OK because this is not for public consumption
    async fn run_safe(self) -> eyre::Result<TxHash>;
    #[allow(async_fn_in_trait)] // OK because this is not for public consumption
    async fn run_with_results_safe(self) -> eyre::Result<TxHash>;
}

impl<P, D> DebugTransaction for CallBuilder<P, D>
where
    P: alloy::providers::Provider + Clone,
    D: alloy::contract::CallDecoder
{
    async fn run_safe(self) -> eyre::Result<TxHash> {
        let provider = self.provider.clone();
        let receipt = self.gas(50_000_000_u64).send().await?.get_receipt().await?;
        if receipt.inner.status() {
            Ok(receipt.transaction_hash)
        } else {
            let default_options = GethDebugTracingOptions::default();
            let _call_options = GethDebugTracingOptions {
                config: GethDefaultTracingOptions {
                    disable_storage: Some(false),
                    disable_stack: Some(false),
                    enable_memory: Some(false),
                    debug: Some(true),
                    ..Default::default()
                },
                tracer: Some(GethDebugTracerType::BuiltInTracer(
                    GethDebugBuiltInTracerType::CallTracer
                )),
                ..Default::default()
            };
            let result = provider
                .debug_trace_transaction(receipt.transaction_hash, default_options)
                .await?;

            println!("TRACE: {result:?}");
            // We can make this do a cool backtrace later
            Err(eyre!("Transaction with hash {} failed", receipt.transaction_hash))
        }
    }

    async fn run_with_results_safe(self) -> eyre::Result<TxHash> {
        let provider = self.provider.clone();
        let receipt = self.gas(50_000_000_u64).send().await?.get_receipt().await?;
        let default_options = GethDebugTracingOptions::default();
        let _call_options = GethDebugTracingOptions {
            config: GethDefaultTracingOptions {
                disable_storage: Some(false),
                disable_stack: Some(false),
                enable_memory: Some(false),
                debug: Some(true),
                ..Default::default()
            },
            tracer: Some(GethDebugTracerType::BuiltInTracer(
                GethDebugBuiltInTracerType::CallTracer
            )),
            ..Default::default()
        };
        let result = provider
            .debug_trace_transaction(receipt.transaction_hash, default_options)
            .await?;

        println!("TRACE: {result:?}");
        // We can make this do a cool backtrace later
        if receipt.inner.status() {
            Ok(receipt.transaction_hash)
        } else {
            Err(eyre!("Transaction with hash {} failed", receipt.transaction_hash))
        }
    }
}

static CREATE_SIGNER: OnceLock<Address> = OnceLock::new();

fn get_or_set_signer(my_address: Address) -> Address {
    *CREATE_SIGNER.get_or_init(|| my_address)
}

pub struct AngstromTestingAddresses {
    pub contract:     Address,
    pub pool_manager: Address,
    pub token0:       Address,
    pub token1:       Address,
    pub hooks:        Address
}
/// deploys the angstrom testhub contract along with two tokens, under the
/// secret key
pub async fn deploy_contract_and_create_pool(
    provider: WalletProviderRpc
) -> eyre::Result<AngstromTestingAddresses> {
    provider
        .anvil_impersonate_account(get_or_set_signer(provider.default_signer_address()))
        .await?;

    let out = anvil_mine_delay(
        Box::pin(async {
            PoolManagerDeployer::deploy(provider.clone(), U256::MAX)
                .await
                .map(|v| v.at(address!("998abeb3e57409262ae5b751f60747921b33613e")))
        }),
        &provider,
        Duration::from_millis(500)
    )
    .await?;

    let v4_address = *out.address();

    let testhub = anvil_mine_delay(
        Box::pin(async {
            TestnetHub::deploy(provider.clone(), Address::ZERO, v4_address)
                .await
                .map(|v| v.at(address!("7969c5ed335650692bc04293b07f5bf2e7a673c0")))
        }),
        &provider,
        Duration::from_millis(500)
    )
    .await?;
    let angstrom_address = *testhub.address();

    // if we don't do these sequentially, the provider nonce messes up and doesn't
    // deploy properly
    let token0 = anvil_mine_delay(
        Box::pin(async {
            MockERC20::deploy(provider.clone())
                .await
                .map(|v| v.at(address!("4ee6ecad1c2dae9f525404de8555724e3c35d07b")))
        }),
        &provider,
        Duration::from_millis(500)
    )
    .await?;
    let token0 = *token0.address();

    let token1 = anvil_mine_delay(
        Box::pin(async {
            MockERC20::deploy(provider.clone())
                .await
                .map(|v| v.at(address!("fbc22278a96299d91d41c453234d97b4f5eb9b2d")))
        }),
        &provider,
        Duration::from_millis(500)
    )
    .await?;
    let token1 = *token1.address();

    tracing::info!(
        ?angstrom_address,
        ?v4_address,
        ?token0,
        ?token1,
        "deployed v4 and angstrom test contract on anvil"
    );
    provider
        .anvil_stop_impersonating_account(get_or_set_signer(provider.default_signer_address()))
        .await?;

    Ok(AngstromTestingAddresses {
        contract: angstrom_address,
        pool_manager: v4_address,
        token0,
        token1,
        hooks: Address::default()
    })
}

// will wait for a specific delay and then call anvil mine wallet.
// this will allow us to quick mine and not have to wait the 12 seconds
// between transactions while avoiding race conditions
pub async fn anvil_mine_delay<F0: Future + Unpin>(
    f0: F0,
    provider: &WalletProviderRpc,
    delay: Duration
) -> F0::Output {
    let mut pinned = pin!(f0);
    if let Ok(v) = tokio::time::timeout(delay, &mut pinned).await {
        return v;
    }
    provider
        .anvil_mine(Some(1), None)
        .await
        .expect("anvil failed to mine");

    pinned.await
}
