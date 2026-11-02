//! The reply readers shared by Codex's app server and Grok Build's ACP stdio, which both speak
//! JSON-RPC over a pipe: telling a reply from the server's own messages, and reading the code and
//! the words of an error reply.

use serde_json::Value;

/// Whether `message` is the reply to the request with `id`: a reply carries the id of its request
/// and no method, while a message with a method is the server's own, whatever id it numbers itself
/// by.
pub(crate) fn is_reply_to(message: &Value, id: i64) -> bool {
    message.get("method").is_none() && message.get("id").and_then(Value::as_i64) == Some(id)
}

/// The code of a JSON-RPC error reply.
pub(crate) fn error_code(reply: &Value) -> Option<i64> {
    reply.pointer("/error/code").and_then(Value::as_i64)
}

/// The `message` of a JSON-RPC error reply on its own, for a caller that tells refusals apart by it;
/// [`error_detail`] is for showing one.
pub(crate) fn error_message(reply: &Value) -> Option<&str> {
    reply.pointer("/error/message").and_then(Value::as_str)
}

/// What a JSON-RPC error reply says, if the reply is one: its message joined to its `data`, since
/// Grok Build refuses an account whose stored credentials have expired with a bare "Internal
/// error" (code -32603) and the cause only in `data` ("Billing service error: Invalid or expired
/// credentials (...)").
pub(crate) fn error_detail(reply: &Value) -> Option<String> {
    let error = reply.get("error")?;
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty());
    let data = error
        .get("data")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|data| !data.is_empty() && Some(*data) != message);
    Some(match (message, data) {
        (Some(message), Some(data)) => format!("{message}: {data}"),
        (Some(text), None) | (None, Some(text)) => text.to_string(),
        (None, None) => "the request was refused".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_reply_is_told_from_the_servers_own_messages() {
        assert!(is_reply_to(&json!({"id": 2, "result": {}}), 2));
        assert!(!is_reply_to(&json!({"id": 2, "result": {}}), 3));
        assert!(!is_reply_to(&json!({"id": 2, "method": "ask"}), 2));
    }

    #[test]
    fn an_error_reads_as_its_message_and_data() {
        let error = |error: Value| error_detail(&json!({"id": 2, "error": error}));
        assert_eq!(error_detail(&json!({"id": 2, "result": {}})), None);
        assert_eq!(
            error(json!({"code": -32603, "message": " Internal error ", "data": "expired"}))
                .as_deref(),
            Some("Internal error: expired")
        );
        assert_eq!(
            error(json!({"message": "busy", "data": "busy"})).as_deref(),
            Some("busy")
        );
        assert_eq!(
            error(json!({"code": 1})).as_deref(),
            Some("the request was refused")
        );
        assert_eq!(
            error_code(&json!({"error": {"code": -32000}})),
            Some(-32000)
        );
        assert_eq!(
            error_message(&json!({"error": {"message": "busy", "data": "x"}})),
            Some("busy")
        );
        assert_eq!(error_message(&json!({"id": 2, "result": {}})), None);
    }
}
