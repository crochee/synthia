//! The interactive [`RunInbox`] seams.
//!
//! Three small helpers. They no-op cleanly when the agent was
//! built without an inbox, so the dispatched loop never has to
//! branch on "is the inbox wired?" at every iteration.

use synthia_provider::{Message, Role};
use tracing::debug;

use super::ReActLoop;
use crate::events::{SteeringSource, SystemEvent};

/// Drain steering from the run inbox and inject it as trailing
/// user turns.
///
/// No-op when no inbox is wired, so the default (inbox-less) run
/// polls nothing and emits nothing.
pub(in crate::agent::re_act) async fn drain_steering(
    this: &ReActLoop,
    messages: &mut Vec<Message>,
) {
    let Some(inbox) = this.inbox.as_ref() else {
        return;
    };
    // The inbox is consumer-supplied (`with_run_inbox`); a panic in it
    // would end the run unreported. Steering is an enhancement, so a
    // failure only costs this iteration's injection.
    let drained = match crate::agent::re_act::catching_panics(
        inbox.take_steering(),
    )
    .await
    {
        Ok(drained) => drained,
        Err(message) => {
            warn_inbox_panic(this, "take_steering", &message);
            return;
        }
    };
    append_injected(this, messages, SteeringSource::Steering, drained);
}

/// Poll follow-up messages at a would-be stop. Empty when no inbox
/// is wired (the run stops as usual).
pub(in crate::agent::re_act) async fn take_follow_ups(
    this: &ReActLoop,
) -> Vec<Message> {
    let Some(inbox) = this.inbox.as_ref() else {
        return Vec::new();
    };
    match crate::agent::re_act::catching_panics(inbox.take_follow_up()).await {
        Ok(follow_ups) => follow_ups,
        Err(message) => {
            warn_inbox_panic(this, "take_follow_up", &message);
            Vec::new()
        }
    }
}

/// Surface an inbox panic as a warning. The run continues: inbox
/// messages are an enhancement, not a precondition for finishing.
fn warn_inbox_panic(this: &ReActLoop, seam: &str, message: &str) {
    use crate::events::WarningKind;
    this.sink.system(SystemEvent::Warning {
        kind: WarningKind::Hook,
        message: format!(
            "run inbox `{seam}` panicked; steering skipped: {message}"
        ),
        iteration: None,
    });
}

/// Append drained inbox messages to the live history and surface
/// the injection as a [`SystemEvent::SteeringInjected`] event.
///
/// Messages are normalized to plain user turns (role `User`, no
/// tool linkage) so an injected message can never break the
/// assistant/tool-result alternation the provider expects. Empty
/// drains are silent.
pub(in crate::agent::re_act) fn append_injected(
    this: &ReActLoop,
    messages: &mut Vec<Message>,
    source: SteeringSource,
    drained: Vec<Message>,
) {
    if drained.is_empty() {
        return;
    }
    let count = drained.len();
    for message in drained {
        messages.push(Message::new(Role::User, message.content));
    }
    debug!(
        ?source,
        count,
        history_len = messages.len(),
        "run inbox: injected messages before next assistant response"
    );
    this.sink
        .system(SystemEvent::SteeringInjected { source, count });
}
