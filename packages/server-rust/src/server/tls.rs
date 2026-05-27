use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::{Path, PathBuf};

use rcgen::{
    BasicConstraints, Certificate, CertificateParams, DnType, DistinguishedName,
    ExtendedKeyUsagePurpose, IsCa, KeyPair, KeyUsagePurpose, SanType,
};

use crate::logger::Logger;

const LEAF_VALIDITY_DAYS: i64 = 30;
const CA_VALIDITY_DAYS: i64 = 365;
const ROTATE_IF_EXPIRES_WITHIN_DAYS: i64 = 3;

const LEAF_VALIDITY_SECS: i64 = LEAF_VALIDITY_DAYS * 24 * 3600;
const CA_VALIDITY_SECS: i64 = CA_VALIDITY_DAYS * 24 * 3600;
const ROTATE_WINDOW_SECS: i64 = ROTATE_IF_EXPIRES_WITHIN_DAYS * 24 * 3600;

#[derive(Debug, Clone)]
pub struct ResolvedHttpsOptions {
    pub https_options: HttpsConfig,
    pub ca_cert_path: Option<PathBuf>,
    pub mode: TlsMode,
}

#[derive(Debug, Clone)]
pub struct HttpsConfig {
    pub key: String,
    pub cert: String,
    pub ca: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TlsMode {
    Provided,
    Generated,
}

pub struct ResolveHttpsOptionsArgs {
    pub https: bool,
    pub tls_cert: Option<String>,
    pub tls_key: Option<String>,
    pub tls_ca: Option<String>,
    pub tls_sans: Option<String>,
    pub host: String,
    pub config_dir: Option<PathBuf>,
    pub logger: Option<Logger>,
}

pub fn resolve_https_options(args: ResolveHttpsOptionsArgs) -> Option<ResolvedHttpsOptions> {
    if !args.https {
        return None;
    }

    // Provided certs take precedence
    if let (Some(cert_path), Some(key_path)) = (&args.tls_cert, &args.tls_key) {
        let cert = fs::read_to_string(cert_path).ok()?;
        let key = fs::read_to_string(key_path).ok()?;
        let ca = args.tls_ca.as_ref().and_then(|p| fs::read_to_string(p).ok());
        return Some(ResolvedHttpsOptions {
            https_options: HttpsConfig { key, cert, ca },
            ca_cert_path: args.tls_ca.map(PathBuf::from),
            mode: TlsMode::Provided,
        });
    }

    // Auto-generate self-signed certs
    let config_dir = args.config_dir?;
    ensure_generated_tls(&args.host, &config_dir, args.tls_sans, args.logger)
}

pub fn is_loopback_host(host: &str) -> bool {
    host == "127.0.0.1" || host == "::1" || host.starts_with("127.")
}

fn ensure_generated_tls(
    host: &str,
    config_dir: &Path,
    tls_sans: Option<String>,
    _logger: Option<Logger>,
) -> Option<ResolvedHttpsOptions> {
    let tls_dir = config_dir.join("tls");
    fs::create_dir_all(&tls_dir).ok()?;

    let ca_key_path = tls_dir.join("ca-key.pem");
    let ca_cert_path = tls_dir.join("ca-cert.pem");
    let server_key_path = tls_dir.join("server-key.pem");
    let server_cert_path = tls_dir.join("server-cert.pem");

    let ca_needs_gen = should_rotate(&ca_cert_path, CA_VALIDITY_SECS) || !ca_key_path.exists();
    let server_needs_gen =
        should_rotate(&server_cert_path, LEAF_VALIDITY_SECS) || !server_key_path.exists();

    if ca_needs_gen || server_needs_gen {
        let (ca_key_pem, ca_cert_pem, server_key_pem, server_cert_pem) =
            generate_tls_certificates(host, tls_sans.as_deref());

        write_pem_file(&ca_key_path, &ca_key_pem);
        write_pem_file(&ca_cert_path, &ca_cert_pem);
        write_pem_file(&server_key_path, &server_key_pem);
        write_pem_file(&server_cert_path, &server_cert_pem);

        tracing::info!(
            component = "tls",
            ca_cert_path = %ca_cert_path.display(),
            cert_path = %server_cert_path.display(),
            "Generated self-signed EmbeddedCowork HTTPS certificate chain"
        );
    }

    let key = fs::read_to_string(&server_key_path).ok()?;
    let cert = fs::read_to_string(&server_cert_path).ok()?;
    let ca_cert = fs::read_to_string(&ca_cert_path).ok()?;
    let chained_cert = format!("{}\n{}\n", cert.trim(), ca_cert.trim());

    Some(ResolvedHttpsOptions {
        https_options: HttpsConfig {
            key,
            cert: chained_cert,
            ca: Some(ca_cert),
        },
        ca_cert_path: Some(ca_cert_path),
        mode: TlsMode::Generated,
    })
}

/// Returns true if the file is older than `max_age_secs - ROTATE_WINDOW_SECS`.
fn should_rotate(path: &Path, max_age_secs: i64) -> bool {
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return true,
    };
    let modified = match metadata.modified() {
        Ok(t) => t,
        Err(_) => return true,
    };
    let age = std::time::SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    age >= max_age_secs - ROTATE_WINDOW_SECS
}

fn write_pem_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, content).expect("Failed to write PEM file");
    // Best-effort chmod on unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
}

fn generate_tls_certificates(
    host: &str,
    tls_sans: Option<&str>,
) -> (String, String, String, String) {
    let now = time::OffsetDateTime::now_utc();

    // Generate self-signed CA certificate
    let ca_key = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)
        .expect("Failed to generate CA key pair");
    let mut ca_params = CertificateParams::new(Vec::<String>::new());
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "EmbeddedCowork Local CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    ca_params.not_before = now - time::Duration::minutes(1);
    ca_params.not_after = now + time::Duration::days(CA_VALIDITY_DAYS);
    let ca_cert = Certificate::from_params(ca_params).expect("Failed to generate CA certificate");

    // Generate self-signed server certificate
    let server_key = KeyPair::generate(&rcgen::PKCS_ECDSA_P256_SHA256)
        .expect("Failed to generate server key pair");
    let dns_names = build_dns_names(host, tls_sans);
    let mut server_params = CertificateParams::new(dns_names);

    server_params.distinguished_name = DistinguishedName::new();
    server_params
        .distinguished_name
        .push(DnType::CommonName, pick_common_name(host));

    for ip in build_ip_sans(host, tls_sans) {
        server_params
            .subject_alt_names
            .push(SanType::IpAddress(ip));
    }

    server_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    server_params.not_before = now - time::Duration::minutes(1);
    server_params.not_after = now + time::Duration::days(LEAF_VALIDITY_DAYS);

    let server_cert = Certificate::from_params(server_params)
        .expect("Failed to generate server certificate");

    (
        ca_key.serialize_pem(),
        ca_cert.serialize_pem().expect("Failed to serialize CA cert"),
        server_key.serialize_pem(),
        server_cert.serialize_pem().expect("Failed to serialize server cert"),
    )
}

fn build_dns_names(host: &str, tls_sans: Option<&str>) -> Vec<String> {
    let mut names = Vec::new();
    names.push("localhost".to_string());

    if host != "0.0.0.0" && !is_ipv4(host) && !host.is_empty() {
        names.push(host.to_string());
    }

    if let Some(sans) = tls_sans {
        for san in sans.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if !is_ipv4(san) && !names.contains(&san.to_string()) {
                names.push(san.to_string());
            }
        }
    }

    names
}

fn build_ip_sans(host: &str, tls_sans: Option<&str>) -> Vec<IpAddr> {
    let mut ips = Vec::new();
    ips.push(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));

    if let Ok(ip) = host.parse::<IpAddr>() {
        if ip != IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)) && !ips.contains(&ip) {
            ips.push(ip);
        }
    }

    if let Some(sans) = tls_sans {
        for san in sans.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if let Ok(ip) = san.parse::<IpAddr>() {
                if !ips.contains(&ip) {
                    ips.push(ip);
                }
            }
        }
    }

    ips
}

fn pick_common_name(host: &str) -> String {
    match host {
        "" | "0.0.0.0" | "127.0.0.1" => "localhost".to_string(),
        _ => host.to_string(),
    }
}

fn is_ipv4(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|part| {
        !part.is_empty()
            && part.chars().all(|c| c.is_ascii_digit())
            && part.len() <= 3
            && part.parse::<u8>().is_ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_ipv4() {
        assert!(is_ipv4("127.0.0.1"));
        assert!(is_ipv4("0.0.0.0"));
        assert!(is_ipv4("192.168.1.1"));
        assert!(!is_ipv4("localhost"));
        assert!(!is_ipv4("256.0.0.1"));
        assert!(!is_ipv4(""));
    }

    #[test]
    fn test_build_dns_names() {
        let names = build_dns_names("0.0.0.0", None);
        assert!(names.contains(&"localhost".to_string()));
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn test_build_ip_sans() {
        let ips = build_ip_sans("127.0.0.1", None);
        assert!(ips.contains(&IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))));
    }
}
