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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_password_and_otp_titles() {
        assert_eq!(redact_title("Enter password"), "[redacted]");
        assert_eq!(redact_title("Google Authenticator"), "[redacted]");
        assert_eq!(redact_title("GitHub - Ftracker"), "GitHub - Ftracker");
    }
}
