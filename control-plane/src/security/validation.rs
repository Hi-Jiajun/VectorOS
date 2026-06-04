use std::net::IpAddr;

/// Maximum allowed string lengths for various fields.
pub const MAX_USERNAME_LEN: usize = 64;
pub const MAX_PASSWORD_LEN: usize = 128;
pub const MAX_INTERFACE_NAME_LEN: usize = 16;
pub const MAX_IP_STRING_LEN: usize = 45; // IPv6 max length
pub const MAX_DESCRIPTION_LEN: usize = 512;
pub const MAX_PATH_LEN: usize = 256;
pub const MAX_HOSTNAME_LEN: usize = 253;

/// Validation error collection.
#[derive(Debug, Clone)]
pub struct ValidationErrors {
    pub errors: Vec<String>,
}

impl ValidationErrors {
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    pub fn add(&mut self, field: &str, message: &str) {
        self.errors
            .push(format!("{}: {}", field, message));
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn merge(&mut self, other: ValidationErrors) {
        self.errors.extend(other.errors);
    }
}

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Validation errors: {}", self.errors.join("; "))
    }
}

/// Validate an IPv4 or IPv6 address string.
pub fn validate_ip_address(ip: &str, field_name: &str) -> Result<(), String> {
    if ip.is_empty() {
        return Err(format!("{}: cannot be empty", field_name));
    }
    if ip.len() > MAX_IP_STRING_LEN {
        return Err(format!(
            "{}: exceeds maximum length of {} characters",
            field_name, MAX_IP_STRING_LEN
        ));
    }
    if ip.parse::<IpAddr>().is_err() {
        return Err(format!("{}: '{}' is not a valid IP address", field_name, ip));
    }
    Ok(())
}

/// Validate a CIDR network prefix (e.g. "192.168.1.0/24").
pub fn validate_cidr(cidr: &str, field_name: &str) -> Result<(), String> {
    if cidr.is_empty() {
        return Err(format!("{}: cannot be empty", field_name));
    }
    let parts: Vec<&str> = cidr.split('/').collect();
    if parts.len() != 2 {
        return Err(format!(
            "{}: '{}' is not a valid CIDR (expected format: ip/prefix)",
            field_name, cidr
        ));
    }
    validate_ip_address(parts[0], field_name)?;
    let prefix_len: u32 = parts[1].parse().map_err(|_| {
        format!(
            "{}: '{}' has invalid prefix length",
            field_name, cidr
        )
    })?;
    // Determine max prefix based on address family
    if parts[0].contains(':') {
        if prefix_len > 128 {
            return Err(format!(
                "{}: IPv6 prefix length must be 0-128, got {}",
                field_name, prefix_len
            ));
        }
    } else if prefix_len > 32 {
        return Err(format!(
            "{}: IPv4 prefix length must be 0-32, got {}",
            field_name, prefix_len
        ));
    }
    Ok(())
}

/// Validate a network port number (1-65535).
pub fn validate_port(port: u32, field_name: &str) -> Result<(), String> {
    if port == 0 || port > 65535 {
        return Err(format!(
            "{}: port must be between 1 and 65535, got {}",
            field_name, port
        ));
    }
    Ok(())
}

/// Validate a port string (may contain ranges like "80,443" or "1024-65535").
pub fn validate_port_string(ports: &str, field_name: &str) -> Result<(), String> {
    if ports.is_empty() {
        return Ok(());
    }
    for part in ports.split(',') {
        let part = part.trim();
        if part.contains('-') {
            // Port range
            let range_parts: Vec<&str> = part.split('-').collect();
            if range_parts.len() != 2 {
                return Err(format!(
                    "{}: '{}' is not a valid port range",
                    field_name, part
                ));
            }
            let start: u32 = range_parts[0].trim().parse().map_err(|_| {
                format!("{}: '{}' contains invalid port number", field_name, part)
            })?;
            let end: u32 = range_parts[1].trim().parse().map_err(|_| {
                format!("{}: '{}' contains invalid port number", field_name, part)
            })?;
            validate_port(start, field_name)?;
            validate_port(end, field_name)?;
            if start > end {
                return Err(format!(
                    "{}: port range start ({}) must be <= end ({})",
                    field_name, start, end
                ));
            }
        } else {
            // Single port or named port (like "http", "https")
            if let Ok(port_num) = part.parse::<u32>() {
                validate_port(port_num, field_name)?;
            }
            // Named ports are allowed (e.g., "http", "https", "ssh")
        }
    }
    Ok(())
}

/// Validate a network interface name.
///
/// Interface names should be alphanumeric with hyphens, dots, or colons (for VLANs).
/// Examples: "eth0", "enp1s0", "wan0", "bond0.100"
pub fn validate_interface_name(name: &str, field_name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err(format!("{}: cannot be empty", field_name));
    }
    if name.len() > MAX_INTERFACE_NAME_LEN {
        return Err(format!(
            "{}: exceeds maximum length of {} characters",
            field_name, MAX_INTERFACE_NAME_LEN
        ));
    }
    // Allow alphanumeric, hyphens, dots (for VLAN sub-interfaces), and colons
    for ch in name.chars() {
        if !ch.is_alphanumeric() && ch != '-' && ch != '.' && ch != ':' && ch != '@' {
            return Err(format!(
                "{}: '{}' contains invalid character '{}'",
                field_name, name, ch
            ));
        }
    }
    // Must start with a letter or known prefix
    if let Some(first) = name.chars().next() {
        if !first.is_alphabetic() {
            return Err(format!(
                "{}: must start with a letter, got '{}'",
                field_name, first
            ));
        }
    }
    Ok(())
}

/// Validate MTU value (typically 576-9216 for most network interfaces).
pub fn validate_mtu(mtu: u32, field_name: &str) -> Result<(), String> {
    if mtu < 576 || mtu > 9216 {
        return Err(format!(
            "{}: MTU must be between 576 and 9216, got {}",
            field_name, mtu
        ));
    }
    Ok(())
}

/// Validate MRU value (typically same range as MTU).
pub fn validate_mru(mru: u32, field_name: &str) -> Result<(), String> {
    if mru < 576 || mru > 9216 {
        return Err(format!(
            "{}: MRU must be between 576 and 9216, got {}",
            field_name, mru
        ));
    }
    Ok(())
}

/// Validate a username string.
pub fn validate_username(username: &str) -> Result<(), String> {
    if username.is_empty() {
        return Err("Username cannot be empty".to_string());
    }
    if username.len() > MAX_USERNAME_LEN {
        return Err(format!(
            "Username exceeds maximum length of {} characters",
            MAX_USERNAME_LEN
        ));
    }
    // Allow alphanumeric, underscore, hyphen, dot
    for ch in username.chars() {
        if !ch.is_alphanumeric() && ch != '_' && ch != '-' && ch != '.' {
            return Err(format!(
                "Username contains invalid character '{}'",
                ch
            ));
        }
    }
    Ok(())
}

/// Validate a password (basic length and complexity check).
pub fn validate_password(password: &str) -> Result<(), String> {
    if password.is_empty() {
        return Err("Password cannot be empty".to_string());
    }
    if password.len() < 8 {
        return Err("Password must be at least 8 characters".to_string());
    }
    if password.len() > MAX_PASSWORD_LEN {
        return Err(format!(
            "Password exceeds maximum length of {} characters",
            MAX_PASSWORD_LEN
        ));
    }
    Ok(())
}

/// Validate a rate/burst value for QoS or traffic shaping.
pub fn validate_rate_value(value: u64, field_name: &str, max: u64) -> Result<(), String> {
    if value == 0 {
        return Err(format!("{}: must be greater than 0", field_name));
    }
    if value > max {
        return Err(format!("{}: must be at most {}, got {}", field_name, max, value));
    }
    Ok(())
}

/// Validate a string is not empty and within length limits.
pub fn validate_non_empty_string(
    value: &str,
    field_name: &str,
    max_len: usize,
) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("{}: cannot be empty", field_name));
    }
    if value.len() > max_len {
        return Err(format!(
            "{}: exceeds maximum length of {} characters",
            field_name, max_len
        ));
    }
    Ok(())
}

/// Validate a hostname or domain name.
pub fn validate_hostname(hostname: &str, field_name: &str) -> Result<(), String> {
    if hostname.is_empty() {
        return Err(format!("{}: cannot be empty", field_name));
    }
    if hostname.len() > MAX_HOSTNAME_LEN {
        return Err(format!(
            "{}: exceeds maximum length of {} characters",
            field_name, MAX_HOSTNAME_LEN
        ));
    }
    // Basic hostname validation: alphanumeric, hyphens, dots
    for ch in hostname.chars() {
        if !ch.is_alphanumeric() && ch != '-' && ch != '.' {
            return Err(format!(
                "{}: '{}' contains invalid character '{}'",
                field_name, hostname, ch
            ));
        }
    }
    // Must not start or end with a hyphen or dot
    if hostname.starts_with('-')
        || hostname.starts_with('.')
        || hostname.ends_with('-')
        || hostname.ends_with('.')
    {
        return Err(format!(
            "{}: must not start or end with a hyphen or dot",
            field_name
        ));
    }
    Ok(())
}

/// Sanitize a string by removing potentially dangerous characters.
///
/// This is a defense-in-depth measure; proper input validation should
/// be applied separately.
pub fn sanitize_string(input: &str) -> String {
    input
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

/// Validate a description field (optional, up to max length).
pub fn validate_description(desc: &str, field_name: &str) -> Result<(), String> {
    if desc.len() > MAX_DESCRIPTION_LEN {
        return Err(format!(
            "{}: exceeds maximum length of {} characters",
            field_name, MAX_DESCRIPTION_LEN
        ));
    }
    Ok(())
}

/// Validate an IP address or CIDR string (for firewall rules).
pub fn validate_ip_or_cidr(value: &str, field_name: &str) -> Result<(), String> {
    if value.is_empty() {
        return Ok(());
    }
    // Try as plain IP first
    if validate_ip_address(value, field_name).is_ok() {
        return Ok(());
    }
    // Try as CIDR
    if validate_cidr(value, field_name).is_ok() {
        return Ok(());
    }
    Err(format!(
        "{}: '{}' is not a valid IP address or CIDR",
        field_name, value
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ip() {
        assert!(validate_ip_address("192.168.1.1", "test").is_ok());
        assert!(validate_ip_address("::1", "test").is_ok());
        assert!(validate_ip_address("not-an-ip", "test").is_err());
        assert!(validate_ip_address("", "test").is_err());
    }

    #[test]
    fn test_validate_cidr() {
        assert!(validate_cidr("192.168.1.0/24", "test").is_ok());
        assert!(validate_cidr("10.0.0.0/8", "test").is_ok());
        assert!(validate_cidr("::1/128", "test").is_ok());
        assert!(validate_cidr("192.168.1.0/33", "test").is_err());
        assert!(validate_cidr("192.168.1.0", "test").is_err());
    }

    #[test]
    fn test_validate_port() {
        assert!(validate_port(80, "test").is_ok());
        assert!(validate_port(65535, "test").is_ok());
        assert!(validate_port(0, "test").is_err());
        assert!(validate_port(65536, "test").is_err());
    }

    #[test]
    fn test_validate_interface_name() {
        assert!(validate_interface_name("eth0", "test").is_ok());
        assert!(validate_interface_name("enp1s0", "test").is_ok());
        assert!(validate_interface_name("bond0.100", "test").is_ok());
        assert!(validate_interface_name("eth0:0", "test").is_ok());
        assert!(validate_interface_name("", "test").is_err());
        assert!(validate_interface_name("123abc", "test").is_err());
    }

    #[test]
    fn test_validate_mtu() {
        assert!(validate_mtu(1500, "test").is_ok());
        assert!(validate_mtu(9000, "test").is_ok());
        assert!(validate_mtu(576, "test").is_ok());
        assert!(validate_mtu(575, "test").is_err());
        assert!(validate_mtu(9217, "test").is_err());
    }
}
