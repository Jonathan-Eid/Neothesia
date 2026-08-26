use std::cell::RefCell;

use song_analysis::LlmUsage;

use super::{AnalysisBackend, BackendError, SpanRequest, SpanResponse};

/// A backend that returns a fixed, pre-scripted response set - so tests
/// exercise the escalation/validation pipeline without ever shelling out to
/// a real `claude` process.
pub struct FixedBackend {
    pub responses: RefCell<Option<Vec<SpanResponse>>>,
    pub calls: RefCell<u32>,
}

impl FixedBackend {
    pub fn new(responses: Vec<SpanResponse>) -> Self {
        Self {
            responses: RefCell::new(Some(responses)),
            calls: RefCell::new(0),
        }
    }
}

impl AnalysisBackend for FixedBackend {
    fn resolve_spans(
        &self,
        _requests: &[SpanRequest],
    ) -> Result<(Vec<SpanResponse>, LlmUsage), BackendError> {
        *self.calls.borrow_mut() += 1;
        Ok((
            self.responses.borrow_mut().take().unwrap_or_default(),
            LlmUsage::default(),
        ))
    }
}
