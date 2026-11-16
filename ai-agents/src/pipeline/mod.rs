//! Pipeline de pasos sobre un estado compartido.
//!
//! Cada paso hace una sola cosa y decide si el pipeline sigue o se detiene.
//! Agregar comportamiento (por ejemplo, describir imágenes con un modelo de
//! visión) es escribir un paso nuevo y agregarlo en la lista del agente.

use std::sync::Arc;

use async_trait::async_trait;

use crate::shared::error::BoxError;

/// Qué hace el pipeline después de un paso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Flow {
    Continue,
    /// Detenerse sin error (por ejemplo, "no hay que responder").
    Halt(String),
}

/// Un paso del pipeline.
#[async_trait]
pub trait Step<S: Send>: Send + Sync {
    /// Nombre corto para los logs.
    fn name(&self) -> &'static str;

    async fn run(&self, state: &mut S) -> Result<Flow, BoxError>;
}

/// Cómo terminó una ejecución.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Halted { step: &'static str, reason: String },
}

/// Un paso falló con error.
#[derive(Debug, thiserror::Error)]
#[error("step `{step}` failed: {source}")]
pub struct StepFailure {
    pub step: &'static str,
    #[source]
    pub source: BoxError,
}

/// Lista ordenada de pasos.
pub struct Pipeline<S> {
    steps: Vec<Arc<dyn Step<S>>>,
}

impl<S: Send> Pipeline<S> {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn then(mut self, step: impl Step<S> + 'static) -> Self {
        self.steps.push(Arc::new(step));
        self
    }

    pub fn step_names(&self) -> Vec<&'static str> {
        self.steps.iter().map(|step| step.name()).collect()
    }

    pub async fn run(&self, state: &mut S) -> Result<Outcome, StepFailure> {
        for step in &self.steps {
            let name = step.name();
            match step.run(state).await {
                Ok(Flow::Continue) => tracing::trace!(step = name, "step done"),
                Ok(Flow::Halt(reason)) => return Ok(Outcome::Halted { step: name, reason }),
                Err(source) => return Err(StepFailure { step: name, source }),
            }
        }
        Ok(Outcome::Completed)
    }
}

impl<S: Send> Default for Pipeline<S> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Push(&'static str, Flow);

    #[async_trait]
    impl Step<Vec<&'static str>> for Push {
        fn name(&self) -> &'static str {
            self.0
        }

        async fn run(&self, state: &mut Vec<&'static str>) -> Result<Flow, BoxError> {
            state.push(self.0);
            Ok(self.1.clone())
        }
    }

    struct Fail;

    #[async_trait]
    impl Step<Vec<&'static str>> for Fail {
        fn name(&self) -> &'static str {
            "fail"
        }

        async fn run(&self, _: &mut Vec<&'static str>) -> Result<Flow, BoxError> {
            Err("boom".into())
        }
    }

    #[tokio::test]
    async fn runs_steps_in_order_until_halt() {
        let pipeline = Pipeline::new()
            .then(Push("a", Flow::Continue))
            .then(Push("b", Flow::Halt("listo".into())))
            .then(Push("c", Flow::Continue));
        let mut state = Vec::new();
        let outcome = pipeline.run(&mut state).await.unwrap();
        assert_eq!(state, vec!["a", "b"]);
        assert_eq!(outcome, Outcome::Halted { step: "b", reason: "listo".into() });
        assert_eq!(pipeline.step_names(), vec!["a", "b", "c"]);
    }

    #[tokio::test]
    async fn reports_the_failing_step() {
        let pipeline = Pipeline::new().then(Push("a", Flow::Continue)).then(Fail);
        let failure = pipeline.run(&mut Vec::new()).await.unwrap_err();
        assert_eq!(failure.step, "fail");
    }
}
