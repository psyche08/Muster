//! Shared limits and validation, applied before sending and again on receipt.
use crate::v1::AgentAttachment;

pub const MAX_BYTES: usize = 6 * 1024 * 1024;
pub const MAX_COUNT: usize = 8;

pub fn validate(text: &str, attachments: &[AgentAttachment]) -> Result<(), String> {
    if text.len() > 64 * 1024 {
        return Err("Message exceeds 64 KiB".into());
    }
    if text.trim().is_empty() && attachments.is_empty() {
        return Err("Add a message or attachment".into());
    }
    if attachments.len() > MAX_COUNT {
        return Err(format!("At most {MAX_COUNT} attachments per message"));
    }
    let mut size = 0usize;
    for attachment in attachments {
        let name = &attachment.name;
        if name.is_empty()
            || name.len() > 240
            || name == "."
            || name == ".."
            || name
                .chars()
                .any(|c| c.is_control() || c == '/' || c == '\\')
        {
            return Err("Attachment has an invalid filename".into());
        }
        if attachment.media_type.len() > 128
            || attachment.media_type.chars().any(|c| c.is_control())
        {
            return Err("Attachment has an invalid media type".into());
        }
        size = size
            .checked_add(attachment.data.len())
            .ok_or("Attachments are too large")?;
        if size > MAX_BYTES {
            return Err("Attachments exceed 6 MiB total; choose smaller files".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_paths_and_oversized_batches_but_accepts_image_only_messages() {
        let mut a = AgentAttachment {
            name: "截图.png".into(),
            media_type: "image/png".into(),
            data: vec![1],
        };
        assert!(validate("", &[a.clone()]).is_ok());
        for name in ["../secret", "a\\b", "\nfile", ".", ".."] {
            a.name = name.into();
            assert!(validate("hello", &[a.clone()]).is_err());
        }
        a.name = "file.png".into();
        assert!(validate("", &vec![a.clone(); MAX_COUNT + 1]).is_err());
        a.data = vec![0; MAX_BYTES / 2 + 1];
        assert!(validate("", &[a.clone(), a]).is_err());
        assert!(validate("", &[]).is_err());
    }
}
