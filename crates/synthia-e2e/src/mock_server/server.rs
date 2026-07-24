use std::sync::{
    Arc,
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use anyhow::Result;

use super::types::{MockError, ScriptedResponse};

/// Internal state for the mock server
struct MockState {
    /// Queue of scripted responses to return in order
    response_queue: Vec<ScriptedResponse>,
    /// Index into the response queue
    current_index: usize,
    /// After this many calls, start returning 429 errors
    rate_limit_after: Option<usize>,
    /// Track how many times `next_response` has been called
    call_count: AtomicUsize,
    /// Whether to log incoming requests (for test assertions)
    logged_requests: Vec<String>,
}

impl MockState {
    fn new() -> Self {
        Self {
            response_queue: vec![],
            current_index: 0,
            rate_limit_after: None,
            call_count: AtomicUsize::new(0),
            logged_requests: vec![],
        }
    }

    fn next_response(&mut self) -> Result<(u16, String)> {
        let call_num = self.call_count.fetch_add(1, Ordering::SeqCst);

        // Check rate limit
        if let Some(limit) = self.rate_limit_after
            && call_num >= limit
        {
            let err = MockError::rate_limit(Some(30));
            let json = err.to_json()?;
            return Ok((429, json));
        }

        if self.current_index >= self.response_queue.len() {
            anyhow::bail!(
                "Mock LLM response queue exhausted (called {} times, {} responses queued)",
                call_num + 1,
                self.response_queue.len()
            );
        }

        let scripted = &self.response_queue[self.current_index];
        self.current_index += 1;

        let json = scripted.to_json()?;

        // Log the response for test assertions
        self.logged_requests.push(json.clone());

        let status = if let Some(error) = &scripted.error {
            error.status
        } else {
            200
        };

        Ok((status, json))
    }
}

/// Mock LLM server that returns scripted responses
///
/// This is a lightweight mock that can be used in e2e tests to simulate
/// LLM API behavior without making real network calls.
pub struct MockLlmServer {
    state: Arc<Mutex<MockState>>,
}

impl MockLlmServer {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MockState::new())),
        }
    }

    /// Set a single scripted response (convenience for one-shot tests)
    pub fn set_response(&self, response: ScriptedResponse) {
        let mut state = self.state.lock().unwrap();
        state.response_queue = vec![response];
    }

    /// Queue multiple scripted responses to be returned in sequence
    pub fn set_scripted_responses(&self, responses: Vec<ScriptedResponse>) {
        let mut state = self.state.lock().unwrap();
        state.response_queue = responses;
        state.current_index = 0;
    }

    /// Convenience: queue a single response
    pub fn set_scripted_response(&self, response: ScriptedResponse) {
        self.set_scripted_responses(vec![response]);
    }

    /// Configure rate limiting: return success for the first `n` calls,
    /// then return 429 for all subsequent calls
    pub fn set_rate_limit_after(&self, n: usize) {
        let mut state = self.state.lock().unwrap();
        state.rate_limit_after = Some(n);
    }

    /// Get the next response from the queue. Returns (status_code, json_body).
    pub fn get_response(&self) -> Result<(u16, String)> {
        let mut state = self.state.lock().unwrap();
        state.next_response()
    }

    /// Get the number of calls made so far
    pub fn call_count(&self) -> usize {
        self.state.lock().unwrap().call_count.load(Ordering::SeqCst)
    }

    /// Get all logged responses (for test assertions)
    pub fn logged_responses(&self) -> Vec<String> {
        self.state.lock().unwrap().logged_requests.clone()
    }

    /// Reset the server state (clears response queue and call count)
    pub fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.response_queue.clear();
        state.current_index = 0;
        state.rate_limit_after = None;
        state.call_count = AtomicUsize::new(0);
        state.logged_requests.clear();
    }
}

impl Default for MockLlmServer {
    fn default() -> Self {
        Self::new()
    }
}
