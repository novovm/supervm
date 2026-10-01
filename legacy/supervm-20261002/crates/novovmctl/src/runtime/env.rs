pub fn resolve_audit_file(explicit: Option<&str>) -> Option<String> {
    if let Some(value) = explicit {
        return Some(value.to_string());
    }
    std::env::var("NOVOVMCTL_AUDIT_FILE").ok()
}
