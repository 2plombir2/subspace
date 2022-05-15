use std::sync::Arc;

use async_trait::async_trait;
use subspace_archiving::archiver::ArchivedSegment;
use subspace_core_primitives::BlockNumber;
use subspace_rpc_primitives::{
    BlockSignature, BlockSigningInfo, FarmerMetadata, SlotInfo, SolutionResponse,
};
use tokio::sync::mpsc::Receiver;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;

use subspace_farmer::{
    RpcClient, RpcClientError as MockError, SegmentPipelineEvent, SegmentPipelineEventSender,
};

/// Client mock for benching purpose
#[derive(Clone, Debug)]
pub struct BenchRpcClient {
    inner: Arc<Inner>,
}

#[derive(Debug)]
pub struct Inner {
    metadata: FarmerMetadata,
    acknowledge_archived_segment_sender: mpsc::Sender<u64>,
    archived_segments_receiver: Arc<Mutex<mpsc::Receiver<ArchivedSegment>>>,
    segment_pipeline_event_sender: mpsc::UnboundedSender<SegmentPipelineEvent>,
    segment_producer_handle: Mutex<JoinHandle<()>>,
}

impl BenchRpcClient {
    /// Create a new instance of [`BenchRpcClient`].
    pub fn new(
        metadata: FarmerMetadata,
        mut archived_segments_receiver: mpsc::Receiver<ArchivedSegment>,
        segment_pipeline_event_sender: mpsc::UnboundedSender<SegmentPipelineEvent>,
    ) -> Self {
        let (inner_archived_segments_sender, inner_archived_segments_receiver) = mpsc::channel(10);
        let (acknowledge_archived_segment_sender, mut acknowledge_archived_segment_receiver) =
            mpsc::channel(1);

        let segment_producer_handle = tokio::spawn({
            async move {
                while let Some(segment) = archived_segments_receiver.recv().await {
                    if inner_archived_segments_sender.send(segment).await.is_err() {
                        break;
                    }
                    if acknowledge_archived_segment_receiver.recv().await.is_none() {
                        break;
                    }
                }
            }
        });

        Self {
            inner: Arc::new(Inner {
                metadata,
                archived_segments_receiver: Arc::new(Mutex::new(inner_archived_segments_receiver)),
                segment_pipeline_event_sender,
                acknowledge_archived_segment_sender,
                segment_producer_handle: Mutex::new(segment_producer_handle),
            }),
        }
    }

    pub async fn stop(self) {
        self.inner.segment_producer_handle.lock().await.abort();
    }
}

#[async_trait]
impl RpcClient for BenchRpcClient {
    async fn farmer_metadata(&self) -> Result<FarmerMetadata, MockError> {
        Ok(self.inner.metadata.clone())
    }

    async fn best_block_number(&self) -> Result<BlockNumber, MockError> {
        // Doesn't matter for tests (at least yet)
        Ok(BlockNumber::MAX)
    }

    async fn subscribe_slot_info(&self) -> Result<mpsc::Receiver<SlotInfo>, MockError> {
        unreachable!("Unreachable, as we don't start farming for benchmarking")
    }

    async fn submit_solution_response(
        &self,
        _solution_response: SolutionResponse,
    ) -> Result<(), MockError> {
        unreachable!("Unreachable, as we don't start farming for benchmarking")
    }

    async fn subscribe_block_signing(&self) -> Result<Receiver<BlockSigningInfo>, MockError> {
        unreachable!("Unreachable, as we don't start farming for benchmarking")
    }

    async fn submit_block_signature(
        &self,
        _block_signature: BlockSignature,
    ) -> Result<(), MockError> {
        unreachable!("Unreachable, as we don't start farming for benchmarking")
    }

    async fn subscribe_archived_segments(&self) -> Result<Receiver<ArchivedSegment>, MockError> {
        let (sender, receiver) = mpsc::channel(10);
        let archived_segments_receiver = self.inner.archived_segments_receiver.clone();
        tokio::spawn(async move {
            while let Some(archived_segment) = archived_segments_receiver.lock().await.recv().await
            {
                if sender.send(archived_segment).await.is_err() {
                    break;
                }
            }
        });

        Ok(receiver)
    }

    async fn acknowledge_archived_segment(&self, segment_index: u64) -> Result<(), MockError> {
        self.inner
            .acknowledge_archived_segment_sender
            .send(segment_index)
            .await?;
        Ok(())
    }

    fn segment_pipeline_event_sender(&self) -> SegmentPipelineEventSender {
        SegmentPipelineEventSender::Some(self.inner.segment_pipeline_event_sender.clone())
    }
}
