// Copyright 2026 The Consort contributors
// SPDX-License-Identifier: AGPL-3.0-only

//! What is worth interrupting somebody for.
//!
//! Consort said nothing when it was not the window in front. A message naming
//! you arrived, the gold band was drawn, and unless the window happened to be
//! open on that room nobody found out until they looked. For a client meant to
//! be left running that is the gap that decides whether it can be.
//!
//! ## The rule is not ours
//!
//! Matrix already answers "is this worth a notification", per account, and
//! every other client honours the same answer: push rules. So nothing here
//! decides anything about mutes, or keywords, or whether a direct message
//! counts differently from a room of forty people. Somebody who muted a room
//! in Element finds it muted here, and somebody who added a keyword there gets
//! it here, because the rule being read is theirs rather than one this file
//! invented.
//!
//! The reading is free. matrix-sdk-base runs every timeline event in a sync
//! response through the account's push rules while it is processing that
//! response, which is how its own unread counts are made, and leaves the
//! result on the event. So this is a match on a `Vec<Action>` that is already
//! in hand: no request, no second evaluation, and no way for the two to
//! disagree.
//!
//! ## Nothing about what happened while Consort was closed
//!
//! The first batch of room updates is skipped, and that is deliberate rather
//! than a rounding error. A session restored after an afternoon away catches
//! up in one sync response, and every message anybody sent in that afternoon
//! arrives in it. Announcing all of them at once is forty notifications for
//! news that is hours old, and it is the single most reliable way to make
//! somebody turn notifications off.
//!
//! What was missed is not lost: the room list marks it unread and the line
//! across the conversation says where reading stopped. A notification is for
//! interrupting somebody about something that just happened, and none of that
//! just happened.

use matrix_sdk::deserialized_responses::TimelineEvent;
use matrix_sdk::ruma::push::Action;
use matrix_sdk::{Client, Room};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

/// One thing worth interrupting somebody for.
///
/// A value rather than a drawn notification, because what draws one is the
/// platform and this crate deliberately knows nothing about the platform. The
/// shell turns this into a desktop notification, decides whether to draw it at
/// all, and remembers the room so that clicking it goes somewhere.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    /// Where it happened, so a click can go there.
    pub room_id: String,
    /// What that room is called, which is the heading a notification carries.
    pub room_name: String,
    /// The message itself, so a second notification for the same event can be
    /// recognised as one rather than drawn twice.
    pub event_id: String,
    /// Who said it.
    pub sender: String,
    /// What to call them: their display name in that room, or their user ID
    /// when the room has not told us one. There is always an answer.
    pub sender_name: String,
    /// A line of what they said.
    ///
    /// The plain-text body, never the HTML. A notification is drawn by the
    /// desktop rather than by us, and handing a notification daemon markup it
    /// may or may not render is how a message arrives with tags in it.
    pub body: String,
    /// Whether the push rules called it a highlight, which in practice means
    /// somebody said your name.
    ///
    /// Kept separate from the notification itself because the two are worth
    /// different amounts of interruption, and because a client that treats
    /// every message in every room as a mention is one nobody leaves running.
    pub mention: bool,
    /// Whether the push rules asked for a sound.
    ///
    /// Read from the rule rather than decided here. The default rules ask for
    /// one on a mention and in a direct message and stay quiet in a busy room,
    /// which is the behaviour most people would describe if asked, and it is
    /// already somebody's to change.
    pub sound: bool,
}

/// Watch every room for something worth saying out loud.
///
/// # Lifetime
///
/// Same as [`crate::sync::start`] and the other watchers: the task holds the
/// `Client` and watches a channel belonging to it, so it never ends on its
/// own. The caller owns the handle and aborts it when the session does.
pub fn watch<F>(client: Client, on_notify: F) -> JoinHandle<()>
where
    F: Fn(Notification) + Send + 'static,
{
    tokio::spawn(async move {
        let mut updates = client.subscribe_to_all_room_updates();
        let me = client.user_id().map(ToString::to_string);
        // Whether the catch-up has been through. See the module header for
        // why the first batch says nothing.
        let mut caught_up = false;

        loop {
            let update = match updates.recv().await {
                Ok(update) => update,
                // Too many syncs while this task was busy. What was missed is
                // a notification nobody gets, which is the right way round:
                // the alternative is a burst of them for messages that have
                // since been read.
                Err(RecvError::Lagged(missed)) => {
                    tracing::debug!(missed, "fell behind the sync updates");
                    continue;
                }
                Err(RecvError::Closed) => break,
            };

            if !caught_up {
                caught_up = true;
                continue;
            }

            for (room_id, joined) in &update.joined {
                let Some(room) = client.get_room(room_id) else {
                    // Left from another session between the sync and here.
                    continue;
                };
                for event in &joined.timeline.events {
                    if let Some(one) = worth_saying(&room, event, me.as_deref()).await {
                        on_notify(one);
                    }
                }
            }
        }

        // Only reachable once the client is gone, which cannot happen while
        // this task holds one. Logged rather than ignored so a future change
        // to that is not silent.
        tracing::warn!("the notification watcher ended");
    })
}

/// One event, as something to interrupt somebody with, or nothing.
///
/// Local throughout. Every lookup here reads the store the sync response has
/// just written, so an idle client stays idle and a busy room costs no
/// requests.
async fn worth_saying(
    room: &Room,
    event: &TimelineEvent,
    me: Option<&str>,
) -> Option<Notification> {
    // Before the push rules, because it is the cheaper test and it rejects
    // most events: reactions, receipts, memberships and everything else that
    // is not somebody saying something.
    let message = crate::timeline::facts::message(event)?;

    // Nobody needs telling what they just said. The push rules would refuse
    // this too, and checking here means not reading them to find out.
    if Some(message.sender.as_str()) == me {
        return None;
    }

    // A message this session has no key for. The push rules can still ask for
    // a notification, because the encrypted event matches the rule that
    // notifies for anything encrypted, and there is nothing to put in it: the
    // body is a placeholder saying the key has not arrived. Interrupting
    // somebody to show them that is worse than staying quiet, and the room
    // list still marks the room unread.
    if event.kind.is_utd() {
        return None;
    }

    let actions = event.push_actions()?;
    if !interrupts(actions) {
        return None;
    }

    let sender_name = crate::rooms::facts::name_all(room, std::iter::once(message.sender.as_str()))
        .await
        .into_iter()
        .next()
        .map_or_else(|| message.sender.clone(), |person| person.name);

    Some(Notification {
        room_id: room.room_id().to_string(),
        room_name: crate::rooms::facts::name_of(room).await,
        event_id: message.id,
        sender: message.sender,
        sender_name,
        body: message.body,
        mention: is_mention(actions),
        sound: wants_sound(actions),
    })
}

/// Whether these push actions ask for somebody to be interrupted.
///
/// Its own function, with the two below, because this is the whole of what
/// this module decides and none of it needs a homeserver to check.
fn interrupts(actions: &[Action]) -> bool {
    actions.iter().any(Action::should_notify)
}

/// Whether they say it was about this account.
fn is_mention(actions: &[Action]) -> bool {
    actions.iter().any(Action::is_highlight)
}

/// Whether they ask for a sound.
///
/// Any sound rather than a named one. The tweak carries a name, which on
/// Matrix is almost always `default`, and what a desktop actually plays for it
/// is the desktop's business rather than ours.
fn wants_sound(actions: &[Action]) -> bool {
    actions.iter().any(|action| action.sound().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use matrix_sdk::ruma::push::{HighlightTweakValue, SoundTweakValue, Tweak};

    fn sound() -> Action {
        Action::SetTweak(Tweak::Sound(SoundTweakValue::Default))
    }

    fn highlight() -> Action {
        Action::SetTweak(Tweak::Highlight(HighlightTweakValue::Yes))
    }

    #[test]
    fn an_ordinary_message_in_a_room_worth_watching_interrupts() {
        assert!(interrupts(&[Action::Notify]));
    }

    #[test]
    fn a_muted_room_says_nothing() {
        // Which is what a mute is on the wire: rules that match and produce no
        // notify action. There is no separate mute flag to check, and looking
        // for one would be inventing a second answer to a question the account
        // has already answered.
        assert!(!interrupts(&[]));
    }

    #[test]
    fn a_tweak_on_its_own_is_not_a_reason_to_interrupt() {
        // A rule can set a tweak without notifying. Treating any action at all
        // as a notification would announce every message in every muted room.
        assert!(!interrupts(&[highlight(), sound()]));
    }

    #[test]
    fn somebody_saying_your_name_is_a_mention() {
        assert!(is_mention(&[Action::Notify, highlight()]));
    }

    #[test]
    fn an_ordinary_message_is_not_one() {
        assert!(!is_mention(&[Action::Notify]));
    }

    #[test]
    fn a_highlight_set_to_no_is_not_a_mention() {
        // The rules say this explicitly rather than by omission, and reading
        // the tweak as present-means-true would mark every message in a room
        // whose rules turn highlighting off.
        assert!(!is_mention(&[
            Action::Notify,
            Action::SetTweak(Tweak::Highlight(HighlightTweakValue::No)),
        ]));
    }

    #[test]
    fn a_rule_asking_for_a_sound_gets_one() {
        assert!(wants_sound(&[Action::Notify, sound()]));
    }

    #[test]
    fn a_rule_that_asks_for_none_stays_quiet() {
        assert!(!wants_sound(&[Action::Notify, highlight()]));
    }
}
