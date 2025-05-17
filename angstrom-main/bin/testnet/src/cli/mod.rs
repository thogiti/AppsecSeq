pub mod devnet;
pub mod e2e_orders;
pub mod testnet;
use angstrom_metrics::{METRICS_ENABLED, initialize_prometheus_metrics};
use clap::{ArgAction, Parser, Subcommand};
use devnet::DevnetCli;
use e2e_orders::End2EndOrdersCli;
use reth_tasks::TaskExecutor;
use testing_tools::types::config::{DevnetConfig, TestnetConfig};
use testnet::TestnetCli;
use tracing::Level;
use tracing_subscriber::{
    EnvFilter, Layer, Registry, filter, layer::SubscriberExt, util::SubscriberInitExt
};

use crate::{run_devnet, run_testnet, simulations::e2e_orders::run_e2e_orders};

#[derive(Parser)]
pub struct AngstromTestnetCli {
    /// testnet or devnet commands
    #[clap(subcommand)]
    pub command:      TestnetSubcommmand,
    /// Set the minimum log level.
    ///
    /// -v      Errors
    /// -vv     Warnings
    /// -vvv    Info
    /// -vvvv   Debug
    /// -vvvvv  Traces
    #[clap(short = 'v', long, action = ArgAction::Count, default_value_t = 3, help_heading = "Display", global = true)]
    pub verbosity:    u8,
    /// enables the metrics
    #[clap(long, default_value = "false", global = true)]
    pub metrics:      bool,
    /// spawns the prometheus metrics exporter at the specified port
    /// Default: 6969
    #[clap(long, default_value = "6969", global = true)]
    pub metrics_port: u16
}

impl AngstromTestnetCli {
    pub async fn run_all(executor: TaskExecutor) -> eyre::Result<()> {
        let this = Self::parse();
        this.init_tracing();

        if this.metrics
            && initialize_prometheus_metrics(this.metrics_port)
                .await
                .inspect_err(|e| eprintln!("failed to start metrics endpoint - {:?}", e))
                .is_ok()
        {
            {
                METRICS_ENABLED.set(true).unwrap();
            }
        } else {
            METRICS_ENABLED.set(false).unwrap();
        }
        this.command.run_command(executor).await
    }

    fn init_tracing(&self) {
        init_tracing(self.verbosity);
    }
}

#[derive(Debug, Subcommand, Clone)]
pub enum TestnetSubcommmand {
    #[command(name = "testnet")]
    Testnet(TestnetCli),
    #[command(name = "devnet")]
    Devnet(DevnetCli),
    #[command(name = "e2e")]
    End2EndOrders(End2EndOrdersCli)
}

impl TestnetSubcommmand {
    async fn run_command(self, executor: TaskExecutor) -> eyre::Result<()> {
        match self {
            TestnetSubcommmand::Testnet(testnet_cli) => run_testnet(executor, testnet_cli).await,
            TestnetSubcommmand::Devnet(devnet_cli) => run_devnet(executor, devnet_cli).await,
            TestnetSubcommmand::End2EndOrders(e2e_cli) => run_e2e_orders(executor, e2e_cli).await
        }
    }
}

pub fn init_tracing(verbosity: u8) {
    let level = match verbosity - 1 {
        0 => Level::ERROR,
        1 => Level::WARN,
        2 => Level::INFO,
        3 => Level::DEBUG,
        _ => Level::TRACE
    };

    let envfilter = filter::EnvFilter::builder().try_from_env().ok();
    let format = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_target(true);

    if let Some(f) = envfilter {
        let _ = tracing_subscriber::registry()
            .with(format)
            .with(f)
            .try_init();
    } else {
        let filter = filter::Targets::new()
            .with_target("testnet", level)
            .with_target("devnet", level)
            .with_target("angstrom_rpc", level)
            .with_target("angstrom", level)
            .with_target("testing_tools", level)
            .with_target("angstrom_eth", level)
            .with_target("matching_engine", level)
            .with_target("uniswap_v4", level)
            .with_target("consensus", level)
            .with_target("validation", level)
            .with_target("order_pool", level);
        let _ = tracing_subscriber::registry()
            .with(format)
            .with(filter)
            .try_init();
    }
}

fn layer_builder(filter_str: String) -> Box<dyn Layer<Registry> + Send + Sync> {
    let filter = EnvFilter::builder()
        .with_default_directive(filter_str.parse().unwrap())
        .from_env_lossy();

    tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_target(true)
        .with_filter(filter)
        .boxed()
}
