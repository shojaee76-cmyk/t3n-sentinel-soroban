//! Provider registry + verdict classification (Soroban port).
//!
//! MAINTENANCE CONTRACT: adding a monitored provider = appending ONE entry to
//! `PROVIDERS`. Nothing else changes — no schema migration, no client update.
//!
//! Keeps the same shape as the T3N port and the Solana port (4 providers, the
//! same authenticated GETs) so the on-chain history is directly comparable.

/// A monitored provider. `endpoint` is the cheap authenticated GET that
/// proves the key works.
pub struct ProviderSpec {
    /// Human name used in requests, registry and history.
    pub name: &'static str,
    /// Key name inside the vault provider entry.
    pub secret_key: &'static str,
    /// Cheap authenticated GET that proves the key works.
    pub endpoint: &'static str,
}

pub const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        name: "github",
        secret_key: "github_api_key",
        endpoint: "https://api.github.com/user",
    },
    ProviderSpec {
        name: "groq",
        secret_key: "groq_api_key",
        endpoint: "https://api.groq.com/openai/v1/models",
    },
    ProviderSpec {
        name: "openrouter",
        secret_key: "openrouter_api_key",
        endpoint: "https://openrouter.ai/api/v1/key",
    },
    ProviderSpec {
        name: "openai",
        secret_key: "openai_api_key",
        endpoint: "https://api.openai.com/v1/models",
    },
];

pub fn find(name: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|p| p.name == name)
}

/// Map an HTTP status to a verdict. Pure — unit-testable off-chain.
/// Same shape as the T3N `classify` and the Solana port.
pub fn classify(code: u32) -> (&'static str, &'static str) {
    match code {
        200..=299 => ("VALID", "key accepted by provider"),
        401 | 403 => ("INVALID", "credentials rejected by provider"),
        429 => ("RATE_LIMITED", "quota exhausted — key likely valid"),
        _ => ("UNEXPECTED", "unclassified status code"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_returns_known_providers() {
        assert!(find("github").is_some());
        assert!(find("groq").is_some());
        assert!(find("openrouter").is_some());
        assert!(find("openai").is_some());
    }

    #[test]
    fn find_rejects_unknown_provider() {
        assert!(find("not_a_provider").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn classify_2xx_is_valid() {
        assert_eq!(classify(200), ("VALID", "key accepted by provider"));
        assert_eq!(classify(201), ("VALID", "key accepted by provider"));
        assert_eq!(classify(299), ("VALID", "key accepted by provider"));
    }

    #[test]
    fn classify_401_403_is_invalid() {
        assert_eq!(
            classify(401),
            ("INVALID", "credentials rejected by provider")
        );
        assert_eq!(
            classify(403),
            ("INVALID", "credentials rejected by provider")
        );
    }

    #[test]
    fn classify_429_is_rate_limited() {
        assert_eq!(
            classify(429),
            ("RATE_LIMITED", "quota exhausted — key likely valid")
        );
    }

    #[test]
    fn classify_other_is_unexpected() {
        assert_eq!(classify(500), ("UNEXPECTED", "unclassified status code"));
        assert_eq!(classify(418), ("UNEXPECTED", "unclassified status code"));
    }
}
