pub fn panic_payload_to_string(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::panic_payload_to_string;

    #[test]
    fn panic_payload_to_string_handles_common_payloads() {
        let str_payload: &(dyn std::any::Any + Send) = &"borrowed panic";
        let string_payload: &(dyn std::any::Any + Send) = &String::from("owned panic");
        let unknown_payload: &(dyn std::any::Any + Send) = &42usize;

        assert_eq!(panic_payload_to_string(str_payload), "borrowed panic");
        assert_eq!(panic_payload_to_string(string_payload), "owned panic");
        assert_eq!(
            panic_payload_to_string(unknown_payload),
            "unknown panic payload"
        );
    }
}
