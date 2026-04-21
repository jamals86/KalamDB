//! Shared [`ExecutionPlan`] scaffolding built on the DataFusion 53.x surface.
//!
//! This module intentionally stays thin: it provides helpers that consumers
//! embed inside their own `ExecutionPlan` implementations, instead of forcing a
//! single monolithic plan type across families with very different semantics
//! (MVCC merge, one-shot views, vector TVFs, overlay).

use std::any::Any;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use arrow::array::BooleanArray;
use arrow::compute;
use arrow::record_batch::RecordBatch;
use arrow_schema::SchemaRef;
use crate::stats::single_partition_plan_properties;
use crate::stream::one_shot_batch_stream;
use datafusion::error::{DataFusionError, Result as DataFusionResult};
use datafusion::execution::{SendableRecordBatchStream, TaskContext};
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_plan::{DisplayAs, DisplayFormatType, ExecutionPlan, PlanProperties};

/// Apply provider-side filter, projection, and limit handling to a deferred
/// source batch after the source has materialized its raw rows.
pub fn finalize_deferred_batch(
    mut batch: RecordBatch,
    physical_filter: Option<&Arc<dyn PhysicalExpr>>,
    projection: Option<&[usize]>,
    limit: Option<usize>,
    source_name: &str,
) -> DataFusionResult<RecordBatch> {
    if let Some(predicate) = physical_filter {
        let evaluated = predicate.evaluate(&batch)?.into_array(batch.num_rows())?;
        let Some(mask) = evaluated.as_any().downcast_ref::<BooleanArray>() else {
            return Err(DataFusionError::Execution(format!(
                "{source_name} filter expression did not evaluate to BooleanArray",
            )));
        };
        batch = compute::filter_record_batch(&batch, mask)
            .map_err(|error| DataFusionError::ArrowError(Box::new(error), None))?;
    }

    if let Some(projection) = projection {
        batch = batch
            .project(projection)
            .map_err(|error| DataFusionError::ArrowError(Box::new(error), None))?;
    }

    if let Some(limit) = limit {
        batch = batch.slice(0, limit.min(batch.num_rows()));
    }

    Ok(batch)
}

/// Project a schema with the requested column indices, or return the original
/// schema when no projection was requested.
pub fn projected_schema(
    input_schema: &SchemaRef,
    projection: Option<&[usize]>,
) -> DataFusionResult<SchemaRef> {
    match projection {
        Some(indices) => input_schema
            .project(indices)
            .map(Arc::new)
            .map_err(|error| DataFusionError::ArrowError(Box::new(error), None)),
        None => Ok(Arc::clone(input_schema)),
    }
}

/// Shared handle to the target schema so exec nodes built on top of the
/// shared substrate can share an `Arc<Schema>` instead of cloning it.
pub type SharedSchema = Arc<arrow_schema::Schema>;

/// Deferred source that produces a single [`RecordBatch`] during
/// [`ExecutionPlan::execute`] instead of doing source I/O during planning.
///
/// This is the first shared building block for provider families that can
/// describe their work cheaply in `TableProvider::scan()` and materialize the
/// batch only when execution actually begins.
#[async_trait]
pub trait DeferredBatchSource: Send + Sync {
    fn source_name(&self) -> &'static str;

    fn schema(&self) -> SchemaRef;

    async fn produce_batch(&self) -> DataFusionResult<RecordBatch>;
}

/// Shared execution node for one-shot sources that defer batch creation until
/// execution time.
pub struct DeferredBatchExec {
    source: Arc<dyn DeferredBatchSource>,
    properties: Arc<PlanProperties>,
}

impl DeferredBatchExec {
    pub fn new(source: Arc<dyn DeferredBatchSource>) -> Self {
        let properties = Arc::new(single_partition_plan_properties(source.schema()));
        Self { source, properties }
    }
}

impl fmt::Debug for DeferredBatchExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeferredBatchExec")
            .field("source", &self.source.source_name())
            .finish()
    }
}

impl DisplayAs for DeferredBatchExec {
    fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match t {
            DisplayFormatType::Default | DisplayFormatType::Verbose => {
                write!(f, "DeferredBatchExec: source={}", self.source.source_name())
            },
            DisplayFormatType::TreeRender => write!(f, "source={}", self.source.source_name()),
        }
    }
}

impl ExecutionPlan for DeferredBatchExec {
    fn name(&self) -> &str {
        Self::static_name()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn properties(&self) -> &Arc<PlanProperties> {
        &self.properties
    }

    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }

    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
        if !children.is_empty() {
            return Err(DataFusionError::Execution(
                "DeferredBatchExec does not accept children".to_string(),
            ));
        }
        Ok(self)
    }

    fn execute(
        &self,
        partition: usize,
        _context: Arc<TaskContext>,
    ) -> DataFusionResult<SendableRecordBatchStream> {
        if partition != 0 {
            return Err(DataFusionError::Execution(format!(
                "DeferredBatchExec only supports partition 0, got {partition}",
            )));
        }

        let source = Arc::clone(&self.source);
        let schema = source.schema();
        Ok(one_shot_batch_stream(schema, async move { source.produce_batch().await }))
    }
}
