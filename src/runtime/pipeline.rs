use super::{EngineContext, check_context};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineError {
    Cancelled,
    DeadlineExpired,
}

pub type PipelineStage = Box<dyn Fn(&EngineContext) -> Result<(), PipelineError> + Send + Sync>;

/// Runs context checks before each registered execution stage.
#[derive(Default)]
pub struct ExecutionPipeline {
    stages: Vec<PipelineStage>,
}

impl ExecutionPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_stage<F>(mut self, stage: F) -> Self
    where
        F: Fn(&EngineContext) -> Result<(), PipelineError> + Send + Sync + 'static,
    {
        self.stages.push(Box::new(stage));
        self
    }

    pub fn run(&self, context: &EngineContext) -> Result<(), PipelineError> {
        for stage in &self.stages {
            check_context(context)?;
            stage(context)?;
        }
        check_context(context)
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionPipeline, PipelineError};
    use crate::{
        identity::{CorrelationId, OperationId},
        operation::{Operation, OperationContext},
        runtime::EngineContext,
    };

    fn context() -> EngineContext {
        EngineContext::new(OperationContext::new(Operation::new(
            OperationId::new("pipeline-operation").unwrap(),
            CorrelationId::new("pipeline-correlation").unwrap(),
        )))
    }

    #[test]
    fn pipeline_runs_stages_in_order() {
        let order = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let first_order = order.clone();
        let second_order = order.clone();
        let pipeline = ExecutionPipeline::new()
            .with_stage(move |_| {
                first_order.lock().unwrap().push(1);
                Ok(())
            })
            .with_stage(move |_| {
                second_order.lock().unwrap().push(2);
                Ok(())
            });

        pipeline.run(&context()).unwrap();

        assert_eq!(*order.lock().unwrap(), vec![1, 2]);
    }

    #[test]
    fn pipeline_stops_before_cancelled_stage() {
        let context = context();
        context.cancellation().cancel();
        let pipeline = ExecutionPipeline::new().with_stage(|_| panic!("stage ran"));

        assert_eq!(pipeline.run(&context), Err(PipelineError::Cancelled));
    }
}
