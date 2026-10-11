pub fn redact_title(title: &str) -> String {
    let lower = title.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("passwd")
        || lower.contains("otp")
        || lower.contains("one-time")
        || lower.contains("2fa")
        || lower.contains("authenticator")
    {
        "[redacted]".into()
    } else {
        title.chars().take(180).collect()
    }
}
