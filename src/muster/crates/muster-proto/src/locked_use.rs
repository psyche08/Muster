//! Shared operation binding. Neither an Apple shell nor a plugin constructs it.
use crate::v1::{self, locked_use_operation::Operation, request::Req};
use prost::Message;
use sha2::{Digest, Sha256};

pub fn operation(req: &Req) -> Option<v1::LockedUseOperation> {
    Some(v1::LockedUseOperation {
        provider_queue: String::new(),
        operation: Some(match req {
            Req::AgentSend(r) => Operation::Send(r.clone()),
            Req::AgentRespond(r) => Operation::Respond(r.clone()),
            Req::AgentInterrupt(r) => Operation::Interrupt(r.clone()),
            Req::AgentInspect(r) => Operation::Inspect(r.clone()),
            Req::AgentComplete(r) => Operation::Complete(r.clone()),
            Req::TerminalCreate(r) => Operation::TerminalCreate(r.clone()),
            Req::AgentChats(r) if r.full_history => Operation::ChatLoad(r.clone()),
            Req::AgentChatOpen(r) => Operation::ChatOpen(r.clone()),
            Req::AgentChatOptions(r) => Operation::ChatOptions(r.clone()),
            Req::AgentChatSetOptions(r) => Operation::ChatSetOptions(r.clone()),
            Req::AgentOrganize(r) => Operation::Organize(r.clone()),
            Req::AgentNativeCreate(r) => Operation::NativeCreate(r.clone()),
            _ => return None,
        }),
    })
}

/// The request an operation runs. A terminal lease runs none: it keeps the
/// console unlocked for the terminal requests that follow.
pub fn request(op: &v1::LockedUseOperation) -> Option<Req> {
    Some(match op.operation.as_ref()? {
        Operation::Send(r) => Req::AgentSend(r.clone()),
        Operation::Respond(r) => Req::AgentRespond(r.clone()),
        Operation::Interrupt(r) => Req::AgentInterrupt(r.clone()),
        Operation::Inspect(r) => Req::AgentInspect(r.clone()),
        Operation::Complete(r) => Req::AgentComplete(r.clone()),
        Operation::TerminalCreate(r) => Req::TerminalCreate(r.clone()),
        Operation::ChatLoad(r) => Req::AgentChats(r.clone()),
        Operation::ChatOpen(r) => Req::AgentChatOpen(r.clone()),
        Operation::ChatOptions(r) => Req::AgentChatOptions(r.clone()),
        Operation::ChatSetOptions(r) => Req::AgentChatSetOptions(r.clone()),
        Operation::Organize(r) => Req::AgentOrganize(r.clone()),
        Operation::NativeCreate(r) => Req::AgentNativeCreate(r.clone()),
        Operation::Terminal(_) | Operation::Device(_) => return None,
    })
}

/// A terminal lease for `session`, bound to a fresh `op_id`.
pub fn terminal(op_id: &str, session: &str) -> v1::LockedUseOperation {
    v1::LockedUseOperation {
        provider_queue: String::new(),
        operation: Some(Operation::Terminal(v1::TerminalLeaseRequest {
            op_id: op_id.into(),
            session: session.into(),
        })),
    }
}

pub fn turn(op: &v1::LockedUseOperation) -> Option<&str> {
    let id = match op.operation.as_ref()? {
        Operation::Send(r) => &r.op_id,
        Operation::Respond(r) => &r.op_id,
        Operation::Interrupt(r) => &r.op_id,
        Operation::Inspect(r) => &r.op_id,
        Operation::Complete(r) => &r.op_id,
        Operation::TerminalCreate(r) => &r.op_id,
        Operation::Terminal(r) => &r.op_id,
        Operation::Device(r) => &r.op_id,
        Operation::ChatLoad(r) => &r.op_id,
        Operation::ChatOpen(r) => &r.op_id,
        Operation::ChatOptions(r) => &r.op_id,
        Operation::ChatSetOptions(r) => &r.op_id,
        Operation::Organize(r) => &r.op_id,
        Operation::NativeCreate(r) => &r.op_id,
    };
    (!id.is_empty()
        && id.len() <= 128
        && op.encoded_len() <= crate::attachments::MAX_BYTES + 128 * 1024)
        .then_some(id)
}

pub fn digest(op: &v1::LockedUseOperation) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"MUSTER-LOCKED-OPERATION-V1\0");
    hash.update(op.encode_to_vec());
    hash.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1;
    #[test]
    fn attachment_bytes_and_names_are_bound_to_presence_proof() {
        let mut send = v1::AgentSendRequest {
            op_id: "upload".into(),
            session: "session".into(),
            text: "inspect".into(),
            attachments: vec![v1::AgentAttachment {
                name: "photo.png".into(),
                media_type: "image/png".into(),
                data: vec![1; 256 * 1024],
            }],
            release_unconfirmed: String::new(),
        };
        let original = operation(&Req::AgentSend(send.clone())).unwrap();
        assert_eq!(turn(&original), Some("upload"));
        send.attachments[0].data[0] = 2;
        assert_ne!(
            digest(&original),
            digest(&operation(&Req::AgentSend(send.clone())).unwrap())
        );
        send.attachments[0].data[0] = 1;
        send.attachments[0].name = "other.png".into();
        assert_ne!(
            digest(&original),
            digest(&operation(&Req::AgentSend(send)).unwrap())
        );
    }
    #[test]
    fn binds_operation_kind_session_text_and_id() {
        let base = Req::AgentSend(v1::AgentSendRequest {
            attachments: Vec::new(),
            session: "session-a".into(),
            text: "hello".into(),
            op_id: "turn-1".into(),
            release_unconfirmed: String::new(),
        });
        let original = operation(&base).unwrap();
        assert_eq!(request(&original), Some(base.clone()));
        for changed in [
            Req::AgentSend(v1::AgentSendRequest {
                attachments: Vec::new(),
                session: "session-b".into(),
                text: "hello".into(),
                op_id: "turn-1".into(),
                release_unconfirmed: String::new(),
            }),
            Req::AgentSend(v1::AgentSendRequest {
                attachments: Vec::new(),
                session: "session-a".into(),
                text: "goodbye".into(),
                op_id: "turn-1".into(),
                release_unconfirmed: String::new(),
            }),
            Req::AgentSend(v1::AgentSendRequest {
                attachments: Vec::new(),
                session: "session-a".into(),
                text: "hello".into(),
                op_id: "turn-2".into(),
                release_unconfirmed: String::new(),
            }),
            // A release of an unconfirmed write is the user's decision too.
            Req::AgentSend(v1::AgentSendRequest {
                attachments: Vec::new(),
                session: "session-a".into(),
                text: "hello".into(),
                op_id: "turn-1".into(),
                release_unconfirmed: "app-1-4".into(),
            }),
            Req::AgentInterrupt(v1::AgentInterruptRequest {
                session: "session-a".into(),
                op_id: "turn-1".into(),
            }),
        ] {
            assert_ne!(digest(&original), digest(&operation(&changed).unwrap()));
        }
        assert!(operation(&Req::DeviceInfo(v1::DeviceInfoRequest {})).is_none());
    }
    #[test]
    fn provider_queue_scope_is_part_of_the_signed_operation() {
        let mut op = operation(&Req::AgentInterrupt(v1::AgentInterruptRequest {
            session: "s".into(),
            op_id: "op".into(),
        }))
        .unwrap();
        let single = digest(&op);
        op.provider_queue = "codex".into();
        let codex = digest(&op);
        op.provider_queue = "claude".into();
        assert_ne!(single, codex);
        assert_ne!(codex, digest(&op));
    }
}

#[cfg(test)]
mod inspection_tests {
    use super::*;
    #[test]
    fn completion_is_bound_separately_from_inspection_and_send() {
        let complete = operation(&Req::AgentComplete(v1::AgentCompleteRequest {
            session: "claude-session".into(),
            op_id: "complete-1".into(),
        }))
        .unwrap();
        assert!(matches!(request(&complete), Some(Req::AgentComplete(_))));
        assert_eq!(turn(&complete), Some("complete-1"));
        let inspect = operation(&Req::AgentInspect(v1::AgentInspectRequest {
            session: "claude-session".into(),
            op_id: "complete-1".into(),
        }))
        .unwrap();
        let send = operation(&Req::AgentSend(v1::AgentSendRequest {
            session: "claude-session".into(),
            op_id: "complete-1".into(),
            ..Default::default()
        }))
        .unwrap();
        assert_ne!(digest(&complete), digest(&inspect));
        assert_ne!(digest(&complete), digest(&send));
        let other = operation(&Req::AgentComplete(v1::AgentCompleteRequest {
            session: "another-session".into(),
            op_id: "complete-1".into(),
        }))
        .unwrap();
        assert_ne!(digest(&complete), digest(&other));
    }
    #[test]
    fn inspection_cannot_be_reinterpreted_as_an_answer_or_another_session() {
        let inspect = operation(&Req::AgentInspect(v1::AgentInspectRequest {
            session: "s".into(),
            op_id: "inspect".into(),
        }))
        .unwrap();
        assert_eq!(turn(&inspect), Some("inspect"));
        assert!(matches!(request(&inspect), Some(Req::AgentInspect(_))));
        let send = operation(&Req::AgentSend(v1::AgentSendRequest {
            session: "s".into(),
            op_id: "inspect".into(),
            ..Default::default()
        }))
        .unwrap();
        assert_ne!(digest(&inspect), digest(&send));
        let other = operation(&Req::AgentInspect(v1::AgentInspectRequest {
            session: "other".into(),
            op_id: "inspect".into(),
        }))
        .unwrap();
        assert_ne!(digest(&inspect), digest(&other));
    }
}

#[cfg(test)]
mod chat_tests {
    use super::*;
    #[test]
    fn chat_navigation_and_options_are_complete_signed_operations() {
        let requests = [
            Req::AgentChats(v1::AgentChatsRequest {
                provider: "claude".into(),
                full_history: true,
                op_id: "load".into(),
            }),
            Req::AgentChatOpen(v1::AgentChatOpenRequest {
                provider: "claude".into(),
                chat: "claude-chat-a".into(),
                op_id: "open".into(),
            }),
            Req::AgentChatOptions(v1::AgentChatOptionsRequest {
                session: "claude-chat-a".into(),
                op_id: "options".into(),
            }),
            Req::AgentChatSetOptions(v1::AgentChatSetOptionsRequest {
                session: "claude-chat-a".into(),
                model: "model-a".into(),
                effort: "High".into(),
                op_id: "set".into(),
            }),
        ];
        for req in &requests {
            let op = operation(req).unwrap();
            assert_eq!(request(&op).as_ref(), Some(req));
            assert!(turn(&op).is_some());
            for other in &requests {
                if other != req {
                    assert_ne!(digest(&op), digest(&operation(other).unwrap()));
                }
            }
        }
        let Req::AgentChatSetOptions(base) = &requests[3] else {
            unreachable!()
        };
        for field in 0..4 {
            let mut changed = base.clone();
            match field {
                0 => changed.session = "claude-chat-b".into(),
                1 => changed.model = "model-b".into(),
                2 => changed.effort = "Low".into(),
                _ => changed.op_id = "other-turn".into(),
            }
            assert_ne!(
                digest(&operation(&requests[3]).unwrap()),
                digest(&operation(&Req::AgentChatSetOptions(changed)).unwrap())
            );
        }
        let empty = operation(&Req::AgentChatOptions(
            v1::AgentChatOptionsRequest::default(),
        ))
        .unwrap();
        assert_eq!(turn(&empty), None);
        assert!(operation(&Req::AgentChats(v1::AgentChatsRequest::default())).is_none());
    }
}

#[cfg(test)]
mod creation_tests {
    use super::*;
    #[test]
    fn creation_proof_binds_every_field_and_cannot_become_a_lease() {
        let original = v1::TerminalCreateRequest {
            op_id: "create-1".into(),
            source: "ghostty".into(),
            profile: "profile".into(),
            background: false,
            managed_session: "tmux-owned".into(),
        };
        let op = operation(&Req::TerminalCreate(original.clone())).unwrap();
        assert_eq!(request(&op), Some(Req::TerminalCreate(original.clone())));
        assert_eq!(turn(&op), Some("create-1"));
        assert_ne!(digest(&op), digest(&terminal("create-1", "tmux-owned")));
        for field in 0..5 {
            let mut changed = original.clone();
            match field {
                0 => changed.op_id = "create-2".into(),
                1 => changed.source = "iterm2".into(),
                2 => changed.profile = "other".into(),
                3 => changed.background = true,
                _ => changed.managed_session = "tmux-other".into(),
            }
            assert_ne!(
                digest(&op),
                digest(&operation(&Req::TerminalCreate(changed)).unwrap())
            );
        }
    }
}

#[cfg(test)]
mod organization_tests {
    use super::*;
    #[test]
    fn organization_identity_action_and_destination_are_signed() {
        let request = v1::AgentOrganizeRequest {
            op_id: "operation".into(),
            session: "session-uuid".into(),
            action: v1::agent_organize_request::Action::Move as i32,
            section_id: "native-group".into(),
            name: String::new(),
        };
        let original = operation(&Req::AgentOrganize(request.clone())).unwrap();
        assert_eq!(
            super::request(&original),
            Some(Req::AgentOrganize(request.clone()))
        );
        assert_eq!(turn(&original), Some("operation"));
        for field in 0..4 {
            let mut changed = request.clone();
            match field {
                0 => changed.session = "other-session".into(),
                1 => changed.section_id = "other-group".into(),
                2 => changed.action = v1::agent_organize_request::Action::Archive as i32,
                _ => changed.name = "Other name".into(),
            };
            assert_ne!(
                digest(&original),
                digest(&operation(&Req::AgentOrganize(changed)).unwrap())
            );
        }
    }
}

#[cfg(test)]
mod native_create_tests {
    use super::*;
    #[test]
    fn creation_provider_directory_first_message_and_options_are_signed() {
        let request = v1::AgentNativeCreateRequest {
            op_id: "operation".into(),
            provider: "codex".into(),
            directory: "/Users/example/project".into(),
            prompt: "Summarize the build".into(),
            model: "GPT-6.1 Sol".into(),
            effort: "High".into(),
        };
        let original = operation(&Req::AgentNativeCreate(request.clone())).unwrap();
        assert_eq!(
            super::request(&original),
            Some(Req::AgentNativeCreate(request.clone()))
        );
        assert_eq!(turn(&original), Some("operation"));
        for field in 0..7 {
            let mut changed = request.clone();
            match field {
                0 => changed.op_id = "other-operation".into(),
                1 => changed.provider = "claude".into(),
                2 => changed.directory = "/Users/example/other".into(),
                3 => changed.prompt = "Delete the build".into(),
                4 => changed.model = "GPT-5.5".into(),
                5 => changed.effort = "Low".into(),
                // Leaving the app's choice is a different request too.
                _ => changed.effort.clear(),
            };
            assert_ne!(
                digest(&original),
                digest(&operation(&Req::AgentNativeCreate(changed)).unwrap())
            );
        }
    }
}
