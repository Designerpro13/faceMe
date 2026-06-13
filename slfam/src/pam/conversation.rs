//! PAM conversation functions

use crate::error::{PamError, Result};

/// PAM message style
#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum PamMessageStyle {
    /// Prompt for echo-on input
    PromptEchoOn = 1,
    /// Prompt for echo-off input (password)
    PromptEchoOff = 2,
    /// Error message
    ErrorMsg = 3,
    /// Text info message
    TextInfo = 4,
}

/// PAM conversation message
#[derive(Debug)]
pub struct PamMessage {
    /// Message style
    pub style: PamMessageStyle,
    /// Message text
    pub msg: String,
}

/// PAM conversation response
#[derive(Debug)]
pub struct PamResponse {
    /// Response text (for prompts)
    pub resp: Option<String>,
}

/// Trait for PAM conversation
pub trait PamConversation {
    /// Send a message and optionally get a response
    fn converse(&self, messages: &[PamMessage]) -> Result<Vec<PamResponse>>;
}

/// Display a message to the user
pub fn info_message(conv: &dyn PamConversation, msg: &str) -> Result<()> {
    conv.converse(&[PamMessage {
        style: PamMessageStyle::TextInfo,
        msg: msg.to_string(),
    }])?;
    Ok(())
}

/// Display an error message to the user
pub fn error_message(conv: &dyn PamConversation, msg: &str) -> Result<()> {
    conv.converse(&[PamMessage {
        style: PamMessageStyle::ErrorMsg,
        msg: msg.to_string(),
    }])?;
    Ok(())
}

/// Prompt for input (echo on)
pub fn prompt(conv: &dyn PamConversation, prompt: &str) -> Result<String> {
    let responses = conv.converse(&[PamMessage {
        style: PamMessageStyle::PromptEchoOn,
        msg: prompt.to_string(),
    }])?;

    responses
        .into_iter()
        .next()
        .and_then(|r| r.resp)
        .ok_or_else(|| PamError::ConversationFailed("No response".to_string()).into())
}

/// Prompt for secret input (echo off)
pub fn prompt_secret(conv: &dyn PamConversation, prompt: &str) -> Result<String> {
    let responses = conv.converse(&[PamMessage {
        style: PamMessageStyle::PromptEchoOff,
        msg: prompt.to_string(),
    }])?;

    responses
        .into_iter()
        .next()
        .and_then(|r| r.resp)
        .ok_or_else(|| PamError::ConversationFailed("No response".to_string()).into())
}

/// Null conversation handler (for non-interactive use)
pub struct NullConversation;

impl PamConversation for NullConversation {
    fn converse(&self, _messages: &[PamMessage]) -> Result<Vec<PamResponse>> {
        Ok(vec![])
    }
}

/// Terminal-based conversation handler (for CLI tools)
pub struct TerminalConversation;

impl PamConversation for TerminalConversation {
    fn converse(&self, messages: &[PamMessage]) -> Result<Vec<PamResponse>> {
        use std::io::{self, BufRead, Write};

        let stdin = io::stdin();
        let mut stdout = io::stdout();
        let mut responses = Vec::with_capacity(messages.len());

        for msg in messages {
            match msg.style {
                PamMessageStyle::TextInfo => {
                    println!("{}", msg.msg);
                    responses.push(PamResponse { resp: None });
                }
                PamMessageStyle::ErrorMsg => {
                    eprintln!("Error: {}", msg.msg);
                    responses.push(PamResponse { resp: None });
                }
                PamMessageStyle::PromptEchoOn => {
                    print!("{}", msg.msg);
                    stdout.flush().map_err(|e| {
                        PamError::ConversationFailed(e.to_string())
                    })?;

                    let mut input = String::new();
                    stdin.lock().read_line(&mut input).map_err(|e| {
                        PamError::ConversationFailed(e.to_string())
                    })?;

                    responses.push(PamResponse {
                        resp: Some(input.trim().to_string()),
                    });
                }
                PamMessageStyle::PromptEchoOff => {
                    // In a real implementation, this would disable echo
                    print!("{}", msg.msg);
                    stdout.flush().map_err(|e| {
                        PamError::ConversationFailed(e.to_string())
                    })?;

                    let mut input = String::new();
                    stdin.lock().read_line(&mut input).map_err(|e| {
                        PamError::ConversationFailed(e.to_string())
                    })?;

                    responses.push(PamResponse {
                        resp: Some(input.trim().to_string()),
                    });
                }
            }
        }

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockConversation {
        responses: Vec<Option<String>>,
    }

    impl PamConversation for MockConversation {
        fn converse(&self, messages: &[PamMessage]) -> Result<Vec<PamResponse>> {
            Ok(self
                .responses
                .iter()
                .take(messages.len())
                .map(|r| PamResponse { resp: r.clone() })
                .collect())
        }
    }

    #[test]
    fn test_null_conversation() {
        let conv = NullConversation;
        let messages = vec![PamMessage {
            style: PamMessageStyle::TextInfo,
            msg: "Test".to_string(),
        }];
        let responses = conv.converse(&messages).unwrap();
        assert!(responses.is_empty());
    }

    #[test]
    fn test_mock_conversation() {
        let conv = MockConversation {
            responses: vec![Some("test_response".to_string())],
        };
        let messages = vec![PamMessage {
            style: PamMessageStyle::PromptEchoOn,
            msg: "Input: ".to_string(),
        }];
        let responses = conv.converse(&messages).unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].resp, Some("test_response".to_string()));
    }
}
