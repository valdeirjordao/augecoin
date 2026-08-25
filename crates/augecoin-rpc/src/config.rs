use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, KeyPair};
use std::sync::LazyLock;

struct TestCerts {
    cert_pem: String,
    key_pem: String,
}

static TEST_CERTS: LazyLock<TestCerts> = LazyLock::new(generate_test_certs);

pub fn test_cert_pem() -> &'static str {
    &TEST_CERTS.cert_pem
}

pub fn test_key_pem() -> &'static str {
    &TEST_CERTS.key_pem
}

fn generate_test_certs() -> TestCerts {
    let mut params = CertificateParams::new(vec!["localhost".into()])
        .expect("failed to create certificate params");
    params
        .distinguished_name
        .push(DnType::CommonName, "augecoin-test");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        rcgen::KeyUsagePurpose::DigitalSignature,
        rcgen::KeyUsagePurpose::KeyCertSign,
        rcgen::KeyUsagePurpose::CrlSign,
    ];

    let key_pair = KeyPair::generate().unwrap();
    let cert = params.self_signed(&key_pair).unwrap();

    TestCerts {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    #[test]
    fn test_certs_are_generated() {
        let cert = test_cert_pem();
        let key = test_key_pem();
        assert!(cert.contains("BEGIN CERTIFICATE"));
        assert!(key.contains("BEGIN PRIVATE KEY"));
    }
}
