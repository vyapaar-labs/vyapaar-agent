//! Headless single-turn execution helper (§5.3).

use crate::agent::ChatAgent;
use crate::error::ChatError;
use crate::event::{SessionEvent, TurnOutcome};
use crate::session::SessionSelector;

/// Execute a single conversational turn in headless / non-interactive mode.
///
/// Opens the session, submits user input, drains events until [`SessionEvent::TurnEnded`],
/// and cleanly closes the session handle. Used by CLI (`vyapaar ask`) and background tasks.
pub async fn run_turn_headless(
    agent: &ChatAgent,
    selector: SessionSelector,
    input: impl Into<String>,
) -> Result<(TurnOutcome, Vec<SessionEvent>), ChatError> {
    let (handle, mut events) = agent.open(selector).await?;
    let _turn_id = handle.submit(input).await?;

    let mut collected_events = Vec::new();

    while let Some(event) = events.recv().await {
        if let SessionEvent::TurnEnded { ref outcome, .. } = event {
            let res = outcome.clone();
            collected_events.push(event);
            handle.close().await?;
            return Ok((res, collected_events));
        }
        collected_events.push(event);
    }

    handle.close().await?;
    Ok((TurnOutcome::Completed, collected_events))
}
