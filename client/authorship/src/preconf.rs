use codec::Encode;
use cumulus_client_consensus_proposer::{Error, ProposerInterface};
use cumulus_primitives_parachain_inherent::ParachainInherentData;
use futures::{
    channel::oneshot,
    future::{self, Future, FutureExt},
    SinkExt,
};
use log::{debug, error, info, trace, warn};
use prometheus_endpoint::Registry as PrometheusRegistry;
use sc_basic_authorship::DEFAULT_BLOCK_SIZE_LIMIT;
use sc_block_builder::{BlockBuilder, BlockBuilderApi, BlockBuilderBuilder};
use sc_proposer_metrics::{EndProposingReason, MetricsLink as PrometheusMetrics};
use sc_telemetry::TelemetryHandle;
use sc_transaction_pool_api::{
    InPoolTransaction, ReadyTransactions, TransactionPool, TxInvalidityReportMap,
};
use sp_api::{ApiExt, CallApiAt, ProvideRuntimeApi, StorageProof};
use sp_blockchain::{ApplyExtrinsicFailed, HeaderBackend};
use sp_consensus::{
    DisableProofRecording, EnableProofRecording, Environment, InherentData, ProofRecording,
    Proposal, Proposer,
};
use sp_core::{traits::SpawnNamed, twox_64};
use sp_inherents::InherentDataProvider;
use sp_runtime::{
    traits::{Block as BlockT, Header as HeaderT, One},
    Digest, Percent, SaturatedConversion,
};
use std::{collections::HashSet, sync::LazyLock};
use std::{
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{self, Duration},
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};
use tokio_tungstenite::{accept_async, tungstenite::Message::Text, WebSocketStream};
use tokio_util::sync::CancellationToken;
use warpx_client_primitives::authorship::preconf::{
    PayloadId, PreConfBlockBase, PreConfBlockDiff, PreConfBlockPayload,
};

pub type Subscribers = Arc<Mutex<Vec<WebSocketStream<TcpStream>>>>;
type ReadyTxs<Pool> =
    Box<dyn ReadyTransactions<Item = Arc<<Pool as TransactionPool>::InPoolTransaction>> + Send>;

const LOG_TARGET: &str = "preconf-authorship";

const DEFAULT_SOFT_DEADLINE_PERCENT: Percent = Percent::from_percent(50);
const MAX_SKIPPED_TRANSACTIONS: usize = 8;

pub static PRECONF_SUBSCRIBERS: LazyLock<Subscribers> =
    LazyLock::new(|| Arc::new(Mutex::new(Vec::new())));

/// Start a websocket server to subscribe to block production results.
pub async fn start_preconf_ws_server(addr: &str) {
    info!(target: LOG_TARGET, "Starting websocket server on {}", addr);
    let listener = TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address");
    while let Ok((stream, _)) = listener.accept().await {
        info!("Accepted websocket connection");
        tokio::spawn(async move {
            match accept_async(stream).await {
                Ok(ws_stream) => match PRECONF_SUBSCRIBERS.lock() {
                    Ok(mut subs) => {
                        subs.push(ws_stream);
                        info!("Added websocket connection to subscribers");
                    }
                    Err(e) => {
                        error!("Failed to add websocket connection to subscribers: {}", e);
                    }
                },
                Err(e) => {
                    error!("Failed to accept websocket connection: {}", e);
                }
            }
        });
    }
}

pub struct PreConfProposerFactory<Pool, Client, PR> {
    spawn_handle: Box<dyn SpawnNamed>,
    client: Arc<Client>,
    transaction_pool: Arc<Pool>,
    metrics: PrometheusMetrics,
    default_block_size_limit: usize,
    soft_deadline_percent: Percent,
    telemetry: Option<TelemetryHandle>,
    include_proof_in_block_size_estimation: bool,
    preconf_block_time: u64,
    chain_block_time: u64,
    _phantom: PhantomData<PR>,
}

impl<Pool, Client, PR> Clone for PreConfProposerFactory<Pool, Client, PR> {
    fn clone(&self) -> Self {
        Self {
            spawn_handle: self.spawn_handle.clone(),
            client: self.client.clone(),
            transaction_pool: self.transaction_pool.clone(),
            metrics: self.metrics.clone(),
            default_block_size_limit: self.default_block_size_limit,
            soft_deadline_percent: self.soft_deadline_percent,
            telemetry: self.telemetry.clone(),
            include_proof_in_block_size_estimation: self.include_proof_in_block_size_estimation,
            preconf_block_time: self.preconf_block_time,
            chain_block_time: self.chain_block_time,
            _phantom: self._phantom,
        }
    }
}

impl<Pool, Client> PreConfProposerFactory<Pool, Client, DisableProofRecording> {
    pub fn new(
        spawn_handle: impl SpawnNamed + 'static,
        client: Arc<Client>,
        transaction_pool: Arc<Pool>,
        prometheus: Option<&PrometheusRegistry>,
        telemetry: Option<TelemetryHandle>,
        chain_block_time: u64,
        preconf_block_time: u64,
    ) -> Self {
        Self {
            spawn_handle: Box::new(spawn_handle),
            client,
            transaction_pool,
            metrics: PrometheusMetrics::new(prometheus),
            default_block_size_limit: DEFAULT_BLOCK_SIZE_LIMIT,
            soft_deadline_percent: DEFAULT_SOFT_DEADLINE_PERCENT,
            include_proof_in_block_size_estimation: false,
            telemetry,
            chain_block_time,
            preconf_block_time,
            _phantom: PhantomData,
        }
    }
}

impl<Pool, Client> PreConfProposerFactory<Pool, Client, EnableProofRecording> {
    pub fn with_proof_recording(
        spawn_handle: impl SpawnNamed + 'static,
        client: Arc<Client>,
        transaction_pool: Arc<Pool>,
        prometheus: Option<&PrometheusRegistry>,
        telemetry: Option<TelemetryHandle>,
        chain_block_time: u64,
        preconf_block_time: u64,
    ) -> Self {
        Self {
            spawn_handle: Box::new(spawn_handle),
            client,
            transaction_pool,
            metrics: PrometheusMetrics::new(prometheus),
            default_block_size_limit: DEFAULT_BLOCK_SIZE_LIMIT,
            soft_deadline_percent: DEFAULT_SOFT_DEADLINE_PERCENT,
            include_proof_in_block_size_estimation: true,
            telemetry,
            chain_block_time,
            preconf_block_time,
            _phantom: PhantomData,
        }
    }
}

impl<Pool, Client, PR> PreConfProposerFactory<Pool, Client, PR> {
    pub fn set_default_block_size_limit(&mut self, limit: usize) {
        self.default_block_size_limit = limit;
    }

    pub fn set_soft_deadline(&mut self, percent: Percent) {
        self.soft_deadline_percent = percent;
    }
}

impl<Block, Pool, Client, PR> PreConfProposerFactory<Pool, Client, PR>
where
    Block: BlockT,
    Pool: TransactionPool<Block = Block> + 'static,
    Client: HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + 'static,
    Client::Api: ApiExt<Block> + BlockBuilderApi<Block>,
    PR: ProofRecording,
{
    fn init_with_now(
        &mut self,
        parent_header: &<Block as BlockT>::Header,
        now: Box<dyn Fn() -> time::Instant + Send + Sync>,
    ) -> PreConfProposer<Block, Pool, Client, PR> {
        let parent_hash = parent_header.hash();

        info!(
            "🙌 Starting consensus session on top of parent {:?} (#{})",
            parent_hash.to_string(),
            parent_header.number()
        );

        PreConfProposer::new(
            self.spawn_handle.clone(),
            self.client.clone(),
            self.transaction_pool.clone(),
            parent_hash,
            *parent_header.number(),
            self.metrics.clone(),
            self.telemetry.clone(),
            self.default_block_size_limit,
            self.soft_deadline_percent,
            self.include_proof_in_block_size_estimation,
            self.chain_block_time,
            self.preconf_block_time,
            now,
        )
    }
}

impl<Block, Pool, Client, PR> Environment<Block> for PreConfProposerFactory<Pool, Client, PR>
where
    Block: BlockT,
    Pool: TransactionPool<Block = Block> + 'static,
    Client:
        HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync + 'static,
    Client::Api: ApiExt<Block> + BlockBuilderApi<Block>,
    PR: ProofRecording,
{
    type CreateProposer = future::Ready<Result<Self::Proposer, Self::Error>>;
    type Proposer = PreConfProposer<Block, Pool, Client, PR>;
    type Error = sp_blockchain::Error;

    fn init(&mut self, parent_header: &<Block as BlockT>::Header) -> Self::CreateProposer {
        future::ready(Ok(
            self.init_with_now(parent_header, Box::new(time::Instant::now))
        ))
    }
}

#[async_trait::async_trait]
impl<Block, Pool, Client> ProposerInterface<Block>
    for PreConfProposerFactory<Pool, Client, EnableProofRecording>
where
    Pool: TransactionPool<Block = Block> + 'static,
    Client:
        HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync + 'static,
    Client::Api: ApiExt<Block> + BlockBuilderApi<Block>,
    Block: sp_runtime::traits::Block,
{
    async fn propose(
        &mut self,
        parent_header: &Block::Header,
        paras_inherent_data: &ParachainInherentData,
        other_inherent_data: InherentData,
        inherent_digests: Digest,
        max_duration: Duration,
        block_size_limit: Option<usize>,
    ) -> Result<Option<Proposal<Block, StorageProof>>, Error> {
        let proposer = self
            .init(parent_header)
            .await
            .map_err(|e| Error::proposer_creation(anyhow::Error::new(e)))?;
        let mut inherent_data = other_inherent_data;
        paras_inherent_data
            .provide_inherent_data(&mut inherent_data)
            .await
            .map_err(|e| Error::proposing(anyhow::Error::new(e)))?;
        let proposal = proposer
            .propose(
                inherent_data,
                inherent_digests,
                max_duration,
                block_size_limit,
            )
            .await
            .map_err(|e| Error::proposing(anyhow::Error::new(e)))?;
        Ok(Some(proposal))
    }
}

pub struct PreConfProposer<Block: BlockT, Pool: TransactionPool, Client, PR> {
    spawn_handle: Box<dyn SpawnNamed>,
    client: Arc<Client>,
    parent_hash: Block::Hash,
    parent_number: <<Block as BlockT>::Header as HeaderT>::Number,
    transaction_pool: Arc<Pool>,
    now: Box<dyn Fn() -> time::Instant + Send + Sync>,
    metrics: PrometheusMetrics,
    default_block_size_limit: usize,
    include_proof_in_block_size_estimation: bool,
    soft_deadline_percent: Percent,
    telemetry: Option<TelemetryHandle>,
    pub sub_block_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub chain_block_time: u64,
    pub preconf_block_time: u64,
    _phantom: PhantomData<PR>,
}

impl<Block, Pool, Client, PR> sp_consensus::Proposer<Block>
    for PreConfProposer<Block, Pool, Client, PR>
where
    Block: BlockT,
    Pool: TransactionPool<Block = Block> + 'static,
    Client:
        HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync + 'static,
    Client::Api: ApiExt<Block> + BlockBuilderApi<Block>,
    PR: ProofRecording,
{
    type Proposal =
        Pin<Box<dyn Future<Output = Result<Proposal<Block, PR::Proof>, Self::Error>> + Send>>;
    type Error = sp_blockchain::Error;
    type ProofRecording = PR;
    type Proof = PR::Proof;

    fn propose(
        self,
        inherent_data: sp_consensus::InherentData,
        inherent_digests: sp_runtime::Digest,
        max_duration: time::Duration,
        block_size_limit: Option<usize>,
    ) -> Self::Proposal {
        let (result_tx, result_rx) = oneshot::channel();
        let spawn_handle = self.spawn_handle.clone();
        spawn_handle.spawn_blocking(
            "preconf-authorship-proposer",
            None,
            Box::pin(async move {
                // leave some time for evaluation and block finalization (33%)
                let deadline = (self.now)() + max_duration - max_duration / 3;
                let res = self
                    .propose_with(inherent_data, inherent_digests, deadline, block_size_limit)
                    .await;
                if result_tx.send(res).is_err() {
                    trace!(
                        target: LOG_TARGET,
                        "Could not send block production result to proposer!"
                    );
                }
            }),
        );

        async move { result_rx.await? }.boxed()
    }
}

// Block proposal handler
impl<Block, Pool, Client, PR> PreConfProposer<Block, Pool, Client, PR>
where
    Block: BlockT,
    Pool: TransactionPool<Block = Block> + 'static,
    Client:
        HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync + 'static,
    Client::Api: ApiExt<Block> + BlockBuilderApi<Block>,
    PR: ProofRecording,
{
    async fn propose_with(
        &self,
        inherent_data: sp_consensus::InherentData,
        inherent_digests: sp_runtime::Digest,
        deadline: time::Instant,
        block_size_limit: Option<usize>,
    ) -> Result<Proposal<Block, PR::Proof>, sp_blockchain::Error> {
        let block_timer = time::Instant::now();
        let mut block_builder = BlockBuilderBuilder::new(&*self.client)
            .on_parent_block(self.parent_hash)
            .with_parent_block_number(self.parent_number)
            .with_proof_recording(PR::ENABLED)
            .with_inherent_digests(inherent_digests)
            .build()?;
        self.apply_inherents(&mut block_builder, inherent_data)?;
        // Start loop for preconf block building
        let mut sub_block_count = 0;
        let (builder_tx, mut builder_rx) = mpsc::channel(1);
        let cancel_token = CancellationToken::new();
        let deadline_for_cancel = deadline;
        let cancel_token_clone = cancel_token.clone();
        let block_number = self.parent_number + One::one();
        info!(target: "preconf-authorship", "Starting to propose block #{}", block_number);
        tokio::spawn(async move {
            let now = time::Instant::now();
            if now < deadline_for_cancel {
                tokio::time::sleep_until(tokio::time::Instant::from_std(deadline_for_cancel)).await;
                cancel_token_clone.cancel();
                info!(target: "preconf-authorship", "Deadline reached, stopping proposing for block #{}", block_number);
            }
        });
        let preconf_block_time = self.preconf_block_time;
        let block_size_limit = block_size_limit.unwrap_or(self.default_block_size_limit);
        // Create a task to send authoring signals to the proposer
        let cancel_token_for_interval = cancel_token.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(preconf_block_time));
            loop {
                tokio::select! {
                    _ = cancel_token_for_interval.cancelled() => {
                        info!(target: "preconf-authorship", "Job cancelled during interval, stopping proposing");
                        drop(builder_tx);
                        break;
                    }
                    _ = interval.tick() => {
                        if let Err(err) = builder_tx.send(()).await {
                            error!(target: "preconf-authorship", "Error sending build signal: {}", err);
                            break;
                        }
                    }
                }
            }
        });
        let timeout = deadline.saturating_duration_since((self.now)());
        let mut ready_txs = self
            .transaction_pool
            .ready_at_with_timeout(self.parent_hash, timeout)
            .await;
        debug!(target: LOG_TARGET, "Attempting to push transactions from the pool at {:?}.", self.parent_hash);
        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    info!(target: "preconf-authorship", "Job cancelled during block building, stopping proposing for block #{}", block_number);
                    break;
                }
                received = builder_rx.recv() => {
                    match received {
                        Some(()) => {
                            info!(target: "preconf-authorship", "Building preconf block {}-#{}", block_number, sub_block_count);
                            let base_only_if_first_block = if sub_block_count == 0 {
                                Some(PreConfBlockBase::<Block>::new(
                                    self.parent_hash,
                                    block_number,
                                    block_size_limit,
                                ))
                            } else {
                                None
                            };
                            let preconf_deadline = self.effective_deadline(deadline);
                            if let Some(sub_block_payload) = self
                                .build_sub_block(
                                    &mut block_builder,
                                    preconf_deadline,
                                    block_size_limit,
                                    sub_block_count,
                                    base_only_if_first_block,
                                    &mut ready_txs,
                                )
                                .await? {
                                    let _ = self
                                        .send_message(
                                            serde_json::to_string(&sub_block_payload)
                                                .unwrap_or_default()
                                                .to_string(),
                                        )
                                        .map_err(|e| sp_blockchain::Error::Application(e))?;
                                    // info!(target: "preconf-authorship", "Sent authoring signal for preconf block #{}", sub_block_count);
                                }
                            sub_block_count += 1;
                        }
                        None => break,
                    }
                }
            }
        }
        let (block, storage_changes, proof) = block_builder.build()?.into_inner();
        let block_build_took = block_timer.elapsed();
        let proof =
            PR::into_proof(proof).map_err(|e| sp_blockchain::Error::Application(Box::new(e)))?;
        // self.print_summary(&block, block_build_took, block_timer.elapsed());
        Ok(Proposal {
            block,
            proof,
            storage_changes,
        })
    }

    async fn build_sub_block(
        &self,
        block_builder: &mut BlockBuilder<'_, Block, Client>,
        preconf_deadline: time::Instant,
        block_size_limit: usize,
        sub_block_index: u64,
        preconf_block_base: Option<PreConfBlockBase<Block>>,
        ready_txs: &mut ReadyTxs<Pool>,
    ) -> Result<Option<PreConfBlockPayload<Block, Pool::Hash>>, sp_blockchain::Error> {
        let (end_reason, extrinsics, extrinsics_hashes) = self
            .apply_extrinsics(block_builder, ready_txs, preconf_deadline, block_size_limit)
            .await?;
        let extrinsics_count = extrinsics.len();
        if extrinsics_count > 0 {
            let diff = PreConfBlockDiff::<Block, Pool::Hash> {
                extrinsics,
                hashes: extrinsics_hashes,
            };
            let payload = PreConfBlockPayload::<Block, Pool::Hash> {
                payload_id: Self::new_payload_id(self.parent_hash, sub_block_index),
                index: sub_block_index,
                base: preconf_block_base,
                diff,
                metadata: serde_json::json!({
                    "end_reason": format!("{:?}", end_reason),
                    "extrinsics_count": extrinsics_count,
                }),
            };
            Ok(Some(payload))
        } else {
            Ok(None)
        }
    }

    fn apply_inherents(
        &self,
        block_builder: &mut BlockBuilder<Block, Client>,
        inherent_data: sp_consensus::InherentData,
    ) -> Result<(), sp_blockchain::Error> {
        let create_inherent_start = time::Instant::now();
        let inherents = block_builder.create_inherents(inherent_data)?;
        let create_inherent_took = create_inherent_start.elapsed();
        self.metrics.report(|metrics| {
            metrics
                .create_inherents_time
                .observe(create_inherent_took.as_secs_f64())
        });

        for inherent in inherents {
            match block_builder.push(inherent) {
                Err(sp_blockchain::Error::ApplyExtrinsicFailed(
                    ApplyExtrinsicFailed::Validity(e),
                )) if e.exhausted_resources() => {
                    warn!(
                        target: LOG_TARGET,
                        "⚠️  Dropping non-mandatory inherent from overweight block."
                    )
                }
                Err(sp_blockchain::Error::ApplyExtrinsicFailed(
                    ApplyExtrinsicFailed::Validity(e),
                )) if e.was_mandatory() => {
                    error!(
                        "❌️ Mandatory inherent extrinsic returned error. Block cannot be produced."
                    );
                    return Err(sp_blockchain::Error::ApplyExtrinsicFailed(
                        ApplyExtrinsicFailed::Validity(e),
                    ));
                }
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "❗️ Inherent extrinsic returned unexpected error: {}. Dropping.", e
                    );
                }
                Ok(_) => {}
            }
        }
        Ok(())
    }

    async fn apply_extrinsics(
        &self,
        block_builder: &mut BlockBuilder<'_, Block, Client>,
        ready_txs: &mut ReadyTxs<Pool>,
        deadline: time::Instant,
        block_size_limit: usize,
    ) -> Result<(EndProposingReason, Vec<Block::Extrinsic>, Vec<Pool::Hash>), sp_blockchain::Error>
    {
        let now = (self.now)();
        let left = deadline.saturating_duration_since(now);
        let left_micros: u64 = left.as_micros().saturated_into();
        let soft_deadline =
            now + time::Duration::from_micros(self.soft_deadline_percent.mul_floor(left_micros));
        let mut skipped = 0;
        let mut unqueue_invalid = TxInvalidityReportMap::new();
        let mut transaction_pushed = false;
        let mut extrinsics = Vec::new();
        let mut extrinsics_hashes = Vec::new();

        // Iterate over the txs and push them to the block
        let end_reason = loop {
            let pending_tx = if let Some(pending_tx) = ready_txs.next() {
                pending_tx
            } else {
                debug!(
                    target: LOG_TARGET,
                    "No more transactions, proceeding with proposing."
                );

                break EndProposingReason::NoMoreTransactions;
            };
            let pending_tx_data = (**pending_tx.data()).clone();
            let pending_tx_hash = pending_tx.hash().clone();

            let now = (self.now)();
            if now > deadline {
                info!(
                    target: LOG_TARGET,
                    "Consensus deadline reached when pushing block transactions, \
                proceeding with proposing."
                );
                break EndProposingReason::HitDeadline;
            }

            let block_size =
                block_builder.estimate_block_size(self.include_proof_in_block_size_estimation);
            if block_size + pending_tx_data.encoded_size() > block_size_limit {
                info!(target: LOG_TARGET, "Transaction would overflow the block size limit");
                ready_txs.report_invalid(&pending_tx);
                if skipped < MAX_SKIPPED_TRANSACTIONS {
                    skipped += 1;
                    info!(
                        target: LOG_TARGET,
                        "Transaction would overflow the block size limit, \
                     but will try {} more transactions before quitting.",
                        MAX_SKIPPED_TRANSACTIONS - skipped,
                    );
                    continue;
                } else if now < soft_deadline {
                    info!(
                        target: LOG_TARGET,
                        "Transaction would overflow the block size limit, \
                     but we still have time before the soft deadline, so \
                     we will try a bit more."
                    );
                    continue;
                } else {
                    info!(
                        target: LOG_TARGET,
                        "Reached block size limit, proceeding with proposing."
                    );
                    break EndProposingReason::HitBlockSizeLimit;
                }
            }

            trace!(target: LOG_TARGET, "[{:?}] Pushing to the block.", pending_tx_hash);
            // Add tx to the block
            match sc_block_builder::BlockBuilder::push(block_builder, pending_tx_data.clone()) {
                Ok(()) => {
                    transaction_pushed = true;
                    extrinsics_hashes.push(pending_tx_hash);
                    extrinsics.push(pending_tx_data);
                    // trace!(target: LOG_TARGET, "[{:?}] Pushed to the block.", pending_tx_hash);
                }
                Err(sp_blockchain::Error::ApplyExtrinsicFailed(
                    ApplyExtrinsicFailed::Validity(e),
                )) if e.exhausted_resources() => {
                    ready_txs.report_invalid(&pending_tx);
                    if skipped < MAX_SKIPPED_TRANSACTIONS {
                        skipped += 1;
                        info!(target: LOG_TARGET,
                            "Block seems full, but will try {} more transactions before quitting.",
                            MAX_SKIPPED_TRANSACTIONS - skipped,
                        );
                    } else if (self.now)() < soft_deadline {
                        info!(target: LOG_TARGET,
                            "Block seems full, but we still have time before the soft deadline, \
                             so we will try a bit more before quitting."
                        );
                    } else {
                        info!(
                            target: LOG_TARGET,
                            "Reached block weight limit, proceeding with proposing."
                        );
                        break EndProposingReason::HitBlockWeightLimit;
                    }
                }
                Err(e) => {
                    ready_txs.report_invalid(&pending_tx);
                    info!(
                        target: LOG_TARGET,
                        "[{:?}] Invalid transaction: {} at: {}", pending_tx_hash, e, self.parent_hash
                    );

                    let error_to_report = match e {
                        sp_blockchain::Error::ApplyExtrinsicFailed(
                            ApplyExtrinsicFailed::Validity(e),
                        ) => Some(e),
                        _ => None,
                    };

                    unqueue_invalid.insert(pending_tx_hash, error_to_report);
                }
            }
        };

        if matches!(end_reason, EndProposingReason::HitBlockSizeLimit) && !transaction_pushed {
            warn!(
                target: LOG_TARGET,
                "Hit block size limit of `{}` without including any transaction!", block_size_limit,
            );
        }

        self.transaction_pool
            .report_invalid(Some(self.parent_hash), unqueue_invalid)
            .await;
        Ok((end_reason, extrinsics, extrinsics_hashes))
    }

    // fn print_summary(
    //     &self,
    //     block: &Block,
    //     block_took: time::Duration,
    //     propose_took: time::Duration,
    // ) {
    //     todo!()
    // }

    fn new_payload_id<Hash: AsRef<[u8]>>(parent_hash: Hash, index: u64) -> PayloadId {
        let mut data = Vec::new();
        data.extend_from_slice(parent_hash.as_ref());
        data.extend_from_slice(&index.to_le_bytes());
        PayloadId(twox_64(&data))
    }
}

// Message handler
impl<Block, Pool, Client, PR> PreConfProposer<Block, Pool, Client, PR>
where
    Block: BlockT,
    Pool: TransactionPool<Block = Block> + 'static,
    Client:
        HeaderBackend<Block> + ProvideRuntimeApi<Block> + CallApiAt<Block> + Send + Sync + 'static,
    Client::Api: ApiExt<Block> + BlockBuilderApi<Block>,
    PR: ProofRecording,
{
    /// Create a new instance of the PreConfProposer and start a websocket server to subscribe to block production results.
    pub fn new(
        spawn_handle: Box<dyn SpawnNamed>,
        client: Arc<Client>,
        transaction_pool: Arc<Pool>,
        parent_hash: Block::Hash,
        parent_number: <<Block as BlockT>::Header as HeaderT>::Number,
        metrics: PrometheusMetrics,
        telemetry: Option<TelemetryHandle>,
        block_size_limit: usize,
        soft_deadline_percent: Percent,
        include_proof_in_block_size_estimation: bool,
        chain_block_time: u64,
        preconf_block_time: u64,
        now: Box<dyn Fn() -> time::Instant + Send + Sync>,
    ) -> Self {
        let spawn_handle = spawn_handle.clone();
        let (sub_block_tx, sub_block_rx) = tokio::sync::mpsc::unbounded_channel();
        Self::publish_task(sub_block_rx);
        Self {
            spawn_handle,
            client,
            transaction_pool,
            sub_block_tx,
            parent_hash,
            parent_number,
            now,
            metrics,
            default_block_size_limit: block_size_limit,
            include_proof_in_block_size_estimation,
            soft_deadline_percent,
            telemetry,
            chain_block_time,
            preconf_block_time,
            _phantom: PhantomData,
        }
    }

    fn effective_deadline(&self, deadline: time::Instant) -> time::Instant {
        let from_now = (self.now)() + Duration::from_millis(self.preconf_block_time);
        std::cmp::min(from_now, deadline)
    }

    pub fn send_message(
        &self,
        message: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sub_block_tx.send(message).map_err(|e| e.into())
    }

    /// Publish a message to all subscribers.
    fn publish_task(mut sub_block_rx: mpsc::UnboundedReceiver<String>) {
        tokio::spawn(async move {
            while let Some(message) = sub_block_rx.recv().await {
                info!(target: LOG_TARGET, "Publishing message to subscribers: {}", message);
                if let Ok(mut subs) = PRECONF_SUBSCRIBERS.lock() {
                    subs.retain_mut(|ws_stream| {
                        let message = message.clone();
                        async move { ws_stream.send(Text(message.into())).await.is_ok() }
                            .now_or_never()
                            .unwrap_or(false)
                    })
                }
            }
        });
    }
}
