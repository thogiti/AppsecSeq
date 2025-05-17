use std::future::Future;

use alloy::{
    contract::{RawCallBuilder, SolCallBuilder},
    network::{Ethereum, EthereumWallet, Network},
    node_bindings::{Anvil, AnvilInstance},
    providers::{
        Identity, PendingTransaction, Provider, RootProvider, builder,
        fillers::{
            BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
            WalletFiller
        }
    },
    signers::local::PrivateKeySigner
};
use alloy_primitives::Address;
use alloy_sol_types::SolCall;
use angstrom_types::CHAIN_ID;

pub type WalletProviderRpc = FillProvider<
    JoinFill<
        JoinFill<
            Identity,
            JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>
        >,
        WalletFiller<EthereumWallet>
    >,
    RootProvider,
    Ethereum
>;

pub type LocalAnvilRpc = alloy::providers::fillers::FillProvider<
    alloy::providers::fillers::JoinFill<
        alloy::providers::fillers::JoinFill<
            alloy::providers::Identity,
            alloy::providers::fillers::JoinFill<
                alloy::providers::fillers::GasFiller,
                alloy::providers::fillers::JoinFill<
                    alloy::providers::fillers::BlobGasFiller,
                    alloy::providers::fillers::JoinFill<
                        alloy::providers::fillers::NonceFiller,
                        alloy::providers::fillers::ChainIdFiller
                    >
                >
            >
        >,
        alloy::providers::fillers::WalletFiller<EthereumWallet>
    >,
    RootProvider,
    Ethereum
>;

pub async fn spawn_anvil(anvil_key: usize) -> eyre::Result<(AnvilInstance, WalletProviderRpc)> {
    let anvil = Anvil::new()
        .chain_id(CHAIN_ID)
        .arg("--ipc")
        .arg("--code-size-limit")
        .arg("393216")
        .arg("--disable-block-gas-limit")
        .try_spawn()?;

    let endpoint = "/tmp/anvil.ipc";
    tracing::info!(?endpoint);
    let sk: PrivateKeySigner = anvil.keys()[anvil_key].clone().into();

    let wallet = EthereumWallet::new(sk);
    let rpc = builder::<Ethereum>()
        .with_recommended_fillers()
        .wallet(wallet)
        .connect(endpoint)
        .await?;

    tracing::info!("connected to anvil");

    Ok((anvil, rpc))
}

pub(crate) trait SafeDeployPending {
    fn deploy_pending(self) -> impl Future<Output = eyre::Result<PendingTransaction>> + Send;

    fn deploy_pending_creation(
        self,
        nonce: u64,
        from: Address
    ) -> impl Future<Output = eyre::Result<(PendingTransaction, Address)>> + Send;
}

impl<P, N> SafeDeployPending for RawCallBuilder<P, N>
where
    P: Provider<N>,
    N: Network
{
    async fn deploy_pending(self) -> eyre::Result<PendingTransaction> {
        Ok(self.gas(50e6 as u64).send().await?.register().await?)
    }

    async fn deploy_pending_creation(
        mut self,
        nonce: u64,
        from: Address
    ) -> eyre::Result<(PendingTransaction, Address)> {
        self = self.nonce(nonce).from(from);
        let address = self
            .calculate_create_address()
            .expect("transaction is not a contract deployment");

        let pending = self.deploy_pending().await?;

        Ok((pending, address))
    }
}

impl<P, C, N> SafeDeployPending for SolCallBuilder<P, C, N>
where
    P: Provider<N> + Clone,
    C: SolCall + Send + Sync + Clone,
    N: Network
{
    async fn deploy_pending(self) -> eyre::Result<PendingTransaction> {
        Ok(self.gas(50e6 as u64).send().await?.register().await?)
    }

    async fn deploy_pending_creation(
        mut self,
        nonce: u64,
        from: Address
    ) -> eyre::Result<(PendingTransaction, Address)> {
        self = self.nonce(nonce).from(from);
        let address = self
            .calculate_create_address()
            .expect("transaction is not a contract deployment");
        let pending = self.deploy_pending().await?;

        Ok((pending, address))
    }
}
