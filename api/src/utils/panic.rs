use std::any::Any;

/// Best-effort extraction of the message carried by a caught panic payload.
pub fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.as_str()
    } else {
        "<non-string panic payload>"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panic_message_extracts_str_and_string_payloads() {
        let str_payload = std::panic::catch_unwind(|| panic!("static message")).unwrap_err();
        assert_eq!(panic_message(&*str_payload), "static message");

        let string_payload = std::panic::catch_unwind(|| panic!("formatted {}", 42)).unwrap_err();
        assert_eq!(panic_message(&*string_payload), "formatted 42");
    }
}
