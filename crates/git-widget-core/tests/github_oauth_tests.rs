use git_widget_core::github_oauth::{
    parse_device_poll_json, parse_device_start_json, resolve_oauth_client_id, DevicePollResult,
};

#[test]
fn parses_github_device_code_start_response() {
    let raw = r#"{
        "device_code": "3584d83530557fdd1f46af8289938c8ef79f9dc5",
        "user_code": "WDJB-MJHT",
        "verification_uri": "https://github.com/login/device",
        "expires_in": 900,
        "interval": 5
    }"#;
    let parsed = parse_device_start_json(raw).unwrap();
    assert_eq!(parsed.user_code, "WDJB-MJHT");
    assert_eq!(parsed.verification_uri, "https://github.com/login/device");
    assert_eq!(parsed.interval, 5);
}

#[test]
fn parses_github_device_poll_states() {
    let pending = parse_device_poll_json(r#"{"error":"authorization_pending"}"#).unwrap();
    assert_eq!(pending, DevicePollResult::Pending { interval: None });

    let slow = parse_device_poll_json(r#"{"error":"slow_down","interval":10}"#).unwrap();
    assert_eq!(slow, DevicePollResult::Pending { interval: Some(10) });

    let expired = parse_device_poll_json(r#"{"error":"expired_token"}"#).unwrap();
    assert_eq!(expired, DevicePollResult::Expired);

    let denied = parse_device_poll_json(r#"{"error":"access_denied"}"#).unwrap();
    assert_eq!(denied, DevicePollResult::Denied);

    let ok = parse_device_poll_json(r#"{"access_token":"gho_test_123","token_type":"bearer"}"#).unwrap();
    assert_eq!(
        ok,
        DevicePollResult::Authorized {
            access_token: "gho_test_123".to_string()
        }
    );
}

#[test]
fn resolves_oauth_client_id_with_override_priority() {
    assert_eq!(resolve_oauth_client_id(Some("Ov23custom"), None), "Ov23custom");
    assert_eq!(resolve_oauth_client_id(None, Some("Ov23env")), "Ov23env");
    assert!(!resolve_oauth_client_id(None, None).is_empty());
}
