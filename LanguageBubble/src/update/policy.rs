#[derive(Debug, PartialEq, Eq)]
pub(super) struct ReleaseDecision {
    pub save_last_seen: bool,
    pub notify: bool,
}

pub(super) fn decide_release(tag: &str, current: &str, last_seen: &str) -> ReleaseDecision {
    let save_last_seen = is_newer(tag, last_seen);
    ReleaseDecision {
        save_last_seen,
        notify: is_newer(tag, current) && save_last_seen,
    }
}

pub(super) fn extract_tag_name(json: &str) -> Option<String> {
    let idx = json.find("\"tag_name\"")?;
    let rest = &json[idx + 10..];
    let mut chars = rest.chars();
    // skip whitespace and colon
    let mut found_colon = false;
    for c in chars.by_ref() {
        if c == ':' {
            found_colon = true;
            break;
        }
        if !c.is_whitespace() {
            return None;
        }
    }
    if !found_colon {
        return None;
    }
    // skip whitespace and opening quote
    let mut found_quote = false;
    for c in chars.by_ref() {
        if c == '"' {
            found_quote = true;
            break;
        }
        if !c.is_whitespace() {
            return None;
        }
    }
    if !found_quote {
        return None;
    }
    let mut tag = String::new();
    for c in chars {
        if c == '"' {
            return Some(tag);
        }
        tag.push(c);
    }
    None
}

pub(super) fn is_valid_tag(tag: &str) -> bool {
    if tag.is_empty() || tag.len() > 64 {
        return false;
    }
    tag.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn parse_semver(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim_start_matches('v');
    // Strict: any pre-release/build suffix means "doesn't parse cleanly" → caller treats as no-update.
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse::<u32>().ok()?;
    let minor = parts[1].parse::<u32>().ok()?;
    let patch = parts[2].parse::<u32>().ok()?;
    Some((major, minor, patch))
}

pub(super) fn is_newer(latest: &str, current: &str) -> bool {
    if current.is_empty() {
        return true;
    }
    match (parse_semver(latest), parse_semver(current)) {
        (Some(l), Some(c)) => l > c,
        (Some(_), None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_decision_preserves_seen_and_installed_version_rules() {
        for (tag, current, seen, save, notify) in [
            ("v1.2.3", "1.2.2", "1.2.2", true, true),
            ("v1.2.3", "1.2.3", "1.2.2", true, false),
            ("v1.2.3", "1.2.2", "1.2.3", false, false),
            ("1.2.1", "1.2.2", "1.2.2", false, false),
            ("1.2.3-beta", "1.2.2", "", true, false),
            ("vv1.2.3", "invalid", "", true, true),
            ("invalid", "1.2.2", "1.2.1", false, false),
        ] {
            assert_eq!(
                decide_release(tag, current, seen),
                ReleaseDecision {
                    save_last_seen: save,
                    notify
                }
            );
        }
    }

    #[test]
    fn tag_name_extraction_accepts_expected_github_json() {
        assert_eq!(
            extract_tag_name(r#"{"name":"Release","tag_name":"v1.2.3"}"#),
            Some("v1.2.3".to_string())
        );
        assert_eq!(
            extract_tag_name("{\n  \"tag_name\" : \"0.4.1\"\n}"),
            Some("0.4.1".to_string())
        );
    }

    #[test]
    fn tag_name_extraction_rejects_missing_or_malformed_fields() {
        assert_eq!(extract_tag_name(r#"{"name":"v1.2.3"}"#), None);
        assert_eq!(extract_tag_name(r#"{"tag_name":1}"#), None);
        assert_eq!(extract_tag_name(r#"{"tag_name":"v1.2.3}"#), None);
    }

    #[test]
    fn release_tags_are_validated() {
        for tag in ["0.4.1", "v1.2.3", "1.2.3-beta"] {
            assert!(is_valid_tag(tag), "{tag} should be valid");
        }
        for tag in ["", "1.2.3 beta", "1.2.3/asset", "1.2.3_"] {
            assert!(!is_valid_tag(tag), "{tag} should be invalid");
        }
        assert!(!is_valid_tag(&"a".repeat(65)));
    }

    #[test]
    fn semantic_versions_parse_strictly() {
        assert_eq!(parse_semver("0.4.0"), Some((0, 4, 0)));
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        for version in ["1.2", "1.2.3.4", "1.2.3-beta", "v", "1.two.3"] {
            assert_eq!(parse_semver(version), None);
        }
    }

    #[test]
    fn version_comparison_preserves_update_behavior() {
        assert!(is_newer("0.4.1", "0.4.0"));
        assert!(is_newer("1.0.0", "0.4.0"));
        assert!(is_newer("0.4.0", ""));
        assert!(!is_newer("0.4.0", "0.4.0"));
        assert!(!is_newer("0.3.9", "0.4.0"));
        assert!(!is_newer("0.4.1-beta", "0.4.0"));
    }
}
