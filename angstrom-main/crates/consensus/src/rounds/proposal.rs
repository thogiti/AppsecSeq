use std::{
    collections::HashSet,
    task::{Context, Poll, Waker},
    time::{Duration, Instant}
};

use alloy::providers::Provider;
use angstrom_network::manager::StromConsensusEvent;
use angstrom_types::{
    consensus::{PreProposalAggregation, Proposal},
    contract_payloads::angstrom::{AngstromBundle, BundleGasDetails},
    orders::PoolSolution,
    sol_bindings::rpc_orders::AttestAngstromBlockEmpty
};
use futures::{FutureExt, StreamExt, future::BoxFuture};
use matching_engine::MatchingEngineHandle;

use super::{ConsensusState, SharedRoundState};
use crate::rounds::{ConsensusMessage, preproposal_wait_trigger::LastRoundInfo};

type MatchingEngineFuture = BoxFuture<'static, eyre::Result<(Vec<PoolSolution>, BundleGasDetails)>>;

/// Proposal State.
///
/// We only transition to Proposal state if we are the leader.
/// In this state we build the proposal, submit it on chain and then propagate
/// it once its landed on chain. We only submit after it has landed on chain as
/// in the case of inclusion games. the proposal will just be dropped and there
/// is no need for others to verify.
pub struct ProposalState {
    matching_engine_future: Option<MatchingEngineFuture>,
    submission_future:      Option<BoxFuture<'static, Result<bool, tokio::task::JoinError>>>,
    pre_proposal_aggs:      Vec<PreProposalAggregation>,
    proposal:               Option<Proposal>,
    last_round_info:        Option<LastRoundInfo>,
    trigger_time:           Instant,
    waker:                  Waker
}

impl ProposalState {
    pub fn new<P, Matching>(
        pre_proposal_aggregation: HashSet<PreProposalAggregation>,
        handles: &mut SharedRoundState<P, Matching>,
        trigger_time: Instant,
        waker: Waker
    ) -> Self
    where
        P: Provider + Unpin + 'static,
        Matching: MatchingEngineHandle
    {
        // queue building future
        waker.wake_by_ref();
        tracing::info!("proposal");

        Self {
            matching_engine_future: Some(
                handles.matching_engine_output(pre_proposal_aggregation.clone())
            ),
            last_round_info: None,
            pre_proposal_aggs: pre_proposal_aggregation.into_iter().collect::<Vec<_>>(),
            submission_future: None,
            proposal: None,
            trigger_time,
            waker
        }
    }

    fn try_build_proposal<P, Matching>(
        &mut self,
        cx: &mut Context<'_>,
        result: eyre::Result<(Vec<PoolSolution>, BundleGasDetails)>,
        handles: &mut SharedRoundState<P, Matching>
    ) -> bool
    where
        P: Provider + Unpin + 'static,
        Matching: MatchingEngineHandle
    {
        self.last_round_info = Some(LastRoundInfo {
            time_to_complete: Instant::now().duration_since(self.trigger_time)
        });

        let provider = handles.provider.clone();
        let signer = handles.signer.clone();
        let target_block = handles.block_height + 1;

        tracing::debug!("starting to build proposal");

        let Ok(possible_bundle) = result.inspect_err(|e| {
            tracing::error!(err=%e,
                "Failed to properly build proposal, THERE SHALL BE NO PROPOSAL THIS BLOCK :("
            )})
            .map(|(pool_solution, gas_info)| {

                let proposal = Proposal::generate_proposal(
                    handles.block_height,
                    &handles.signer,
                    self.pre_proposal_aggs.clone(),
                    pool_solution,
                );

                self.proposal = Some(proposal.clone());
                let snapshot = handles.fetch_pool_snapshot();

                     AngstromBundle::from_proposal(&proposal, gas_info, &snapshot).inspect_err(|e| {
                        tracing::error!(err=%e,
                            "failed to encode angstrom bundle, THERE SHALL BE NO PROPOSAL THIS BLOCK :("
                        );
                    }).ok()


            }) else {
                return false;
            };

        if possible_bundle.is_none() {
            let signed_attestation =
                AttestAngstromBlockEmpty::sign_and_encode(target_block, &signer);
            handles.propagate_message(ConsensusMessage::PropagateEmptyBlockAttestation(
                signed_attestation
            ));
            cx.waker().wake_by_ref();
        }

        let submission_future = Box::pin(async move {
            let Ok(tx_hash) = provider
                .submit_tx(signer, possible_bundle, target_block)
                .await
            else {
                tracing::error!("submission failed");
                return false;
            };

            let Some(tx_hash) = tx_hash else {
                tracing::info!("submitted unlock attestation");
                return true;
            };

            tracing::info!("submitted bundle");
            provider
                .watch_blocks()
                .await
                .unwrap()
                .with_poll_interval(Duration::from_millis(10))
                .into_stream()
                .next()
                .await;

            let included = provider
                .get_transaction_by_hash(tx_hash)
                .await
                .unwrap()
                .is_some();
            tracing::info!(?included, "block tx result");
            included
        });

        self.waker.wake_by_ref();
        self.submission_future = Some(Box::pin(tokio::spawn(submission_future)));

        true
    }
}

impl<P, Matching> ConsensusState<P, Matching> for ProposalState
where
    P: Provider + Unpin + 'static,
    Matching: MatchingEngineHandle
{
    fn on_consensus_message(
        &mut self,
        _: &mut SharedRoundState<P, Matching>,
        _: StromConsensusEvent
    ) {
        // No messages at this point can effect the consensus round and thus are
        // ignored.
    }

    fn poll_transition(
        &mut self,
        handles: &mut SharedRoundState<P, Matching>,
        cx: &mut Context<'_>
    ) -> Poll<Option<Box<dyn ConsensusState<P, Matching>>>> {
        if let Some(mut b_fut) = self.matching_engine_future.take() {
            match b_fut.poll_unpin(cx) {
                Poll::Ready(state) => {
                    if !self.try_build_proposal(cx, state, handles) {
                        // failed to build. we end here.
                        return Poll::Ready(None);
                    }
                }
                Poll::Pending => self.matching_engine_future = Some(b_fut)
            }
        }

        if let Some(mut b_fut) = self.submission_future.take() {
            match b_fut.poll_unpin(cx) {
                Poll::Ready(transaction_landed) => {
                    if transaction_landed.unwrap_or_default() {
                        let proposal = self.proposal.take().unwrap();
                        handles
                            .messages
                            .push_back(ConsensusMessage::PropagateProposal(proposal));
                        cx.waker().wake_by_ref();
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => self.submission_future = Some(b_fut)
            }
        }

        Poll::Pending
    }

    fn last_round_info(&mut self) -> Option<LastRoundInfo> {
        self.last_round_info.take()
    }
}
