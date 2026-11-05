use crate::ui::chat_message::ChatMessage;
use livekit_session::{Message, MessageOrigin};
use std::collections::BTreeMap;

/// Widget: a conversation with an agent, scrolled to the newest message.
pub struct MessageFeed<'a> {
    /// Keyed by [`Message::seq`], which a growing transcript keeps.
    messages: &'a BTreeMap<u64, Message>,
    id_salt: &'a str,
}

impl<'a> MessageFeed<'a> {
    /// `id_salt` keys the scroll position, e.g. by the conversation's id.
    pub fn new(messages: &'a BTreeMap<u64, Message>, id_salt: &'a str) -> Self {
        Self { messages, id_salt }
    }
}

impl egui::Widget for MessageFeed<'_> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.scope(|ui| {
            egui::ScrollArea::vertical()
                .id_salt(("message_feed", self.id_salt))
                .stick_to_bottom(true)
                .auto_shrink(false)
                .show(ui, |ui| {
                    if self.messages.is_empty() {
                        ui.weak("No messages yet");
                    }
                    for message in self.messages.values() {
                        ui.add(
                            ChatMessage::new(speaker(message), &message.text)
                                .local(is_local(message))
                                .pending(!message.is_final),
                        );
                        ui.add_space(4.0);
                    }
                });
        })
        .response
    }
}

fn speaker(message: &Message) -> &str {
    match message.origin {
        MessageOrigin::AgentText | MessageOrigin::AgentTranscript => "Agent",
        MessageOrigin::UserTranscript => "You (transcribed)",
        MessageOrigin::UserInput => "You",
        _ => message.participant_identity.as_str(),
    }
}

fn is_local(message: &Message) -> bool {
    matches!(
        message.origin,
        MessageOrigin::UserTranscript | MessageOrigin::UserInput
    )
}
