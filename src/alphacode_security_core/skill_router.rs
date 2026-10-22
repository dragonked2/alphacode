use serde::{Deserialize, Serialize};

use super::finding::VulnerabilityClass;
use super::hypothesis::Hypothesis;
use super::scope::LiveScope;

/// A security skill descriptor — what it covers and when to use it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecuritySkillDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub family: SkillFamily,
    pub applicable_tech: Vec<String>,
    pub applicable_vuln_classes: Vec<String>,
    pub required_phase: Vec<String>,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillFamily {
    Recon,
    Authentication,
    Authorization,
    WebVuln,
    Logic,
    ModernTargets,
    CodeAnalysis,
    Ctf,
    Reporting,
    Web3,
    ApiSecurity,
    CloudSecurity,
    ContainerSecurity,
    NetworkSecurity,
    Cryptography,
    Forensics,
    MalwareAnalysis,
    SocialEngineering,
    PhysicalSecurity,
    IotSecurity,
    MobileSecurity,
}

impl SkillFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Recon => "recon",
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::WebVuln => "web_vuln",
            Self::Logic => "logic",
            Self::ModernTargets => "modern_targets",
            Self::CodeAnalysis => "code_analysis",
            Self::Ctf => "ctf",
            Self::Reporting => "reporting",
            Self::Web3 => "web3",
            Self::ApiSecurity => "api_security",
            Self::CloudSecurity => "cloud_security",
            Self::ContainerSecurity => "container_security",
            Self::NetworkSecurity => "network_security",
            Self::Cryptography => "cryptography",
            Self::Forensics => "forensics",
            Self::MalwareAnalysis => "malware_analysis",
            Self::SocialEngineering => "social_engineering",
            Self::PhysicalSecurity => "physical_security",
            Self::IotSecurity => "iot_security",
            Self::MobileSecurity => "mobile_security",
        }
    }
}

/// Routing decision: which skills to load for a given context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillRoute {
    pub skills: Vec<String>,
    pub rationale: String,
    pub confidence: f64,
}

/// Routes skills based on live target analysis.
pub struct SkillRouter;

impl SkillRouter {
    /// Determine which skills are relevant given the current scope and hypothesis.
    pub fn route(scope: &LiveScope, hypothesis: Option<&Hypothesis>) -> SkillRoute {
        let mut selected = Vec::new();
        let mut rationale_parts = Vec::new();

        // Phase-based routing
        match scope.mode {
            super::scope::EngagementMode::Ctf => {
                selected.push("ctf".to_string());
                rationale_parts.push("CTF mode active".to_string());
            }
            super::scope::EngagementMode::BugBounty => {
                rationale_parts.push("Bug bounty engagement".to_string());
            }
            _ => {}
        }

        // Technology-based routing.
        //
        // `all_technologies()` aggregates the top-level list *plus* per-host
        // and per-subdomain fingerprints. Reading only `scope.technologies`
        // missed the common case where a fingerprint lands on a discovered
        // host, which silently skipped the entire web3 routing path (and any
        // other technology-specific skill) for exactly the targets most likely
        // to need it.
        for tech in scope.all_technologies() {
            let skills = Self::skills_for_technology(&tech.name);
            for skill in skills {
                if !selected.contains(&skill) {
                    selected.push(skill);
                    rationale_parts.push(format!("Technology: {}", tech.name));
                }
            }
        }

        // Defense-bypass ranking: fingerprints observed during recon are the
        // strongest shortlist signal, so route the top-ranked bypass techniques
        // as directly reachable skills. Ranked from the curated catalog in
        // [`super::bypass`] — deterministic, no model involvement.
        for technique in super::bypass::rank_for_scope(scope).into_iter().take(3) {
            let skill = format!("bypass-{}", technique.id);
            if !selected.contains(&skill) {
                selected.push(skill);
                rationale_parts.push(format!("Bypass: {}", technique.name));
            }
        }

        // Hypothesis-based routing
        if let Some(h) = hypothesis {
            let skills = Self::skills_for_vuln_class(&h.vuln_class);
            for skill in skills {
                if !selected.contains(&skill) {
                    selected.push(skill);
                    rationale_parts.push(format!("Hypothesis: {}", h.description));
                }
            }
        }

        // Endpoint-based routing
        let has_auth_endpoints = scope.discovered_endpoints.iter().any(|e| e.requires_auth);
        if has_auth_endpoints && !selected.contains(&"authentication-analysis".to_string()) {
            selected.push("authentication-analysis".to_string());
            rationale_parts.push("Authenticated endpoints discovered".to_string());
        }

        let has_api_endpoints = scope.discovered_endpoints.iter().any(|e| {
            let p = e.path.to_lowercase();
            p.contains("/api")
                || p.contains("/graphql")
                || p.contains("/rest")
                || p.contains("/v1/")
                || p.contains("/v2/")
        });
        if has_api_endpoints && !selected.contains(&"api-discovery".to_string()) {
            selected.push("api-discovery".to_string());
            rationale_parts.push("API endpoints discovered".to_string());
        }

        // Always include recon if surface is small (needs more discovery)
        if scope.attack_surface_size() < 5 && !selected.contains(&"passive-recon".to_string()) {
            selected.push("passive-recon".to_string());
            rationale_parts.push("Small attack surface, more recon needed".to_string());
        }

        // Compute confidence based on match strength and evidence diversity.
        // More unique signal sources = higher confidence.
        let signal_count = rationale_parts.len();
        let has_mode_signal = scope.mode == super::scope::EngagementMode::Ctf
            || scope.mode == super::scope::EngagementMode::BugBounty;
        let has_tech_signal = !scope.all_technologies().is_empty();
        let has_hypothesis_signal = hypothesis.is_some();
        let has_endpoint_signal = !scope.discovered_endpoints.is_empty();

        let signal_diversity = [
            has_mode_signal,
            has_tech_signal,
            has_hypothesis_signal,
            has_endpoint_signal,
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        // Confidence formula: base 0.2 + 0.15 per signal source, capped at 0.95
        // More diverse evidence = higher confidence in routing decision
        let confidence = if signal_count == 0 {
            0.1
        } else {
            let base = 0.2_f64;
            let per_signal = 0.15_f64;
            let diversity_bonus = signal_diversity as f64 * 0.05;
            (base + per_signal * signal_count as f64 + diversity_bonus).min(0.95)
        };

        SkillRoute {
            skills: selected,
            rationale: rationale_parts.join("; "),
            confidence,
        }
    }

    /// Map a technology name to applicable skills.
    fn skills_for_technology(tech: &str) -> Vec<String> {
        let mut skills = Vec::new();
        let lower = tech.to_lowercase();
        let tokens: Vec<&str> = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|t| !t.is_empty())
            .collect();
        let has_token = |t: &str| tokens.contains(&t);

        // API & Protocol skills
        if lower.contains("graphql") {
            skills.push("graphql".to_string());
            skills.push("graphql-introspection".to_string());
            skills.push("graphql-batch-attack".to_string());
        }
        if lower.contains("grpc") {
            skills.push("grpc".to_string());
            skills.push("grpc-reflection".to_string());
        }
        if lower.contains("websocket") || has_token("ws") {
            skills.push("websocket".to_string());
            skills.push("websocket-hijacking".to_string());
        }
        if lower.contains("rest") || lower.contains("api") {
            skills.push("api-security".to_string());
            skills.push("api-abuse".to_string());
            skills.push("rate-limit-bypass".to_string());
        }
        if lower.contains("soap") || lower.contains("xml-rpc") {
            skills.push("soap-security".to_string());
        }
        if lower.contains("json-rpc") || lower.contains("jsonrpc") {
            skills.push("json-rpc-security".to_string());
        }

        // Frontend skills
        if lower.contains("react")
            || lower.contains("next")
            || lower.contains("vue")
            || lower.contains("angular")
            || lower.contains("svelte")
            || lower.contains("ember")
        {
            skills.push("spa-analysis".to_string());
            skills.push("dom-xss".to_string());
            skills.push("prototype-pollution".to_string());
        }
        if lower.contains("webpack") || lower.contains("vite") || lower.contains("rollup") {
            skills.push("bundler-analysis".to_string());
            skills.push("source-map-leak".to_string());
        }
        if lower.contains("service-worker") || lower.contains("pwa") {
            skills.push("pwa-security".to_string());
        }

        // Defense-stack tradecraft: any fingerprint whose component matches a
        // curated bypass technique routes the direct per-technique skill, and
        // edge/proxy stacks additionally get the WAF-bypass meta-skill.
        for technique in super::bypass::for_technology(lower.as_str()) {
            skills.push(format!("bypass-{}", technique.id));
        }
        if lower.contains("cloudflare")
            || lower.contains("akamai")
            || lower.contains("imperva")
            || lower.contains("modsecurity")
            || lower.contains("aws waf")
            || lower.contains("f5")
            || lower.contains("sucuri")
            || lower.contains("waf")
        {
            skills.push("waf-bypass".to_string());
            skills.push("header-spoofing".to_string());
        }
        if lower.contains("cdn") || lower.contains("cloudfront") || lower.contains("fastly") {
            skills.push("cdn-cache-abuse".to_string());
        }

        // Backend skills
        if lower.contains("node")
            || lower.contains("express")
            || lower.contains("fastify")
            || lower.contains("koa")
        {
            skills.push("javascript-analysis".to_string());
            skills.push("nodejs-security".to_string());
            skills.push("prototype-pollution".to_string());
            skills.push("deserialization".to_string());
        }
        if lower.contains("django") || lower.contains("flask") || lower.contains("fastapi") {
            skills.push("python-security".to_string());
            skills.push("python-deserialization".to_string());
        }
        if lower.contains("rails") || lower.contains("ruby") {
            skills.push("rails-security".to_string());
        }
        if lower.contains("laravel") || lower.contains("symfony") || lower.contains("php") {
            skills.push("php-security".to_string());
        }
        if lower.contains("spring") || lower.contains("java") || lower.contains("kotlin") {
            skills.push("java-security".to_string());
            skills.push("java-deserialization".to_string());
        }
        if lower.contains("dotnet") || lower.contains("csharp") || lower.contains("asp") {
            skills.push("dotnet-security".to_string());
        }
        if lower.contains("golang") || lower.contains("go-") {
            skills.push("go-security".to_string());
        }
        if lower.contains("rust") || lower.contains("actix") || lower.contains("rocket") {
            skills.push("rust-security".to_string());
        }

        // Auth & Identity skills
        if lower.contains("oauth") || lower.contains("oidc") {
            skills.push("oauth-analysis".to_string());
            skills.push("oauth-misconfiguration".to_string());
        }
        if lower.contains("jwt") {
            skills.push("jwt-analysis".to_string());
            skills.push("jwt-attack".to_string());
        }
        if lower.contains("saml") {
            skills.push("saml-analysis".to_string());
            skills.push("saml-injection".to_string());
        }
        if lower.contains("ldap") || lower.contains("ad") || lower.contains("active-directory") {
            skills.push("ldap-security".to_string());
            skills.push("ldap-injection".to_string());
        }
        if lower.contains("kerberos") || lower.contains("ntlm") {
            skills.push("windows-auth-security".to_string());
        }
        if lower.contains("mfa") || lower.contains("2fa") || lower.contains("totp") {
            skills.push("mfa-bypass".to_string());
        }

        // Infrastructure skills
        if lower.contains("kubernetes") || lower.contains("k8s") {
            skills.push("kubernetes".to_string());
            skills.push("kubernetes-rbac".to_string());
            skills.push("kubernetes-escape".to_string());
        }
        if lower.contains("docker") || lower.contains("container") {
            skills.push("containers".to_string());
            skills.push("container-escape".to_string());
        }
        if lower.contains("serverless") || lower.contains("lambda") || lower.contains("function") {
            skills.push("serverless".to_string());
            skills.push("serverless-security".to_string());
        }
        if lower.contains("aws") || lower.contains("azure") || lower.contains("gcp") {
            skills.push("cloud".to_string());
            skills.push("cloud-misconfiguration".to_string());
            skills.push("cloud-metadata-ssrf".to_string());
        }
        if lower.contains("terraform") || lower.contains("pulumi") {
            skills.push("iac-security".to_string());
        }
        if lower.contains("nginx") || lower.contains("apache") || lower.contains("iis") {
            skills.push("web-server-security".to_string());
            skills.push("http-smuggling".to_string());
        }
        if lower.contains("redis") || lower.contains("memcached") {
            skills.push("cache-security".to_string());
            skills.push("cache-poisoning".to_string());
        }
        if lower.contains("elasticsearch") || lower.contains("solr") {
            skills.push("search-engine-security".to_string());
        }
        if lower.contains("kafka") || lower.contains("rabbitmq") {
            skills.push("message-queue-security".to_string());
        }

        // Database skills
        if lower.contains("mysql") || lower.contains("postgres") || lower.contains("mariadb") {
            skills.push("sql-injection".to_string());
            skills.push("database-security".to_string());
        }
        if lower.contains("mongodb") || lower.contains("nosql") {
            skills.push("nosql-injection".to_string());
        }
        if lower.contains("cassandra") || lower.contains("dynamodb") {
            skills.push("nosql-security".to_string());
        }

        // Web3 / EVM stack routes to the dedicated security-research skill,
        // which carries its own domain references (accounting, oracles,
        // bridges, governance, exploit chaining).
        if lower.contains("solidity")
            || has_token("evm")
            || lower.contains("web3")
            || lower.contains("foundry")
            || lower.contains("hardhat")
            || lower.contains("defi")
            || lower.contains("erc20")
            || lower.contains("erc4626")
            || lower.contains("erc721")
            || lower.contains("erc1155")
            // These were a second, separate `if` that re-pushed the same skill.
            // `lower == "solidity"` is already implied by the `contains` above,
            // so tech "solidity" used to load this skill twice.
            || lower == "vyper"
            || lower == "cairo"
        {
            skills.push("web3-security-research".to_string());
        }

        // Mobile skills
        if lower.contains("android") || lower.contains("ios") || lower.contains("mobile") {
            skills.push("mobile-security".to_string());
            skills.push("mobile-api-security".to_string());
        }
        if lower.contains("react-native") || lower.contains("flutter") {
            skills.push("hybrid-app-security".to_string());
        }

        // IoT skills
        if lower.contains("iot") || lower.contains("embedded") || lower.contains("firmware") {
            skills.push("iot-security".to_string());
            skills.push("firmware-analysis".to_string());
        }

        // Network skills
        if lower.contains("tcp") || lower.contains("udp") || lower.contains("network") {
            skills.push("network-security".to_string());
        }
        if lower.contains("dns") {
            skills.push("dns-security".to_string());
            skills.push("dns-rebinding".to_string());
        }
        if lower.contains("vpn") || lower.contains("wireguard") {
            skills.push("vpn-security".to_string());
        }

        // Cryptography skills
        if lower.contains("tls") || lower.contains("ssl") {
            skills.push("tls-security".to_string());
            skills.push("certificate-analysis".to_string());
        }
        if lower.contains("crypto") || lower.contains("encryption") {
            skills.push("cryptography".to_string());
            skills.push("crypto-attack".to_string());
        }

        // Several independent conditions above can match the same technology
        // and push the same skill, and the caller concatenates this list with
        // the vuln-class list. Deduping here keeps order (priority signal)
        // while dropping the repeats.
        let mut seen = std::collections::HashSet::new();
        skills.retain(|skill| seen.insert(skill.clone()));
        skills
    }

    /// Map a vulnerability class to applicable skills.
    fn skills_for_vuln_class(vuln_class: &VulnerabilityClass) -> Vec<String> {
        match vuln_class {
            VulnerabilityClass::Xss | VulnerabilityClass::DomXss => {
                vec!["xss".to_string(), "dom-xss".to_string()]
            }
            VulnerabilityClass::SqlInjection => vec!["sqli".to_string()],
            VulnerabilityClass::NoSqlInjection => vec!["nosqli".to_string()],
            VulnerabilityClass::Ssrf | VulnerabilityClass::ServerSideRequestForgery => {
                vec!["ssrf".to_string()]
            }
            VulnerabilityClass::IdorBola | VulnerabilityClass::InsecureDirectObjectReference => {
                vec!["idor".to_string()]
            }
            VulnerabilityClass::CommandInjection => vec!["command-injection".to_string()],
            VulnerabilityClass::Ssti | VulnerabilityClass::TemplateInjection => {
                vec!["ssti".to_string()]
            }
            VulnerabilityClass::Xxe => vec!["xxe".to_string()],
            VulnerabilityClass::Lfi | VulnerabilityClass::PathTraversal => vec!["lfi".to_string()],
            VulnerabilityClass::Csrf | VulnerabilityClass::ClientSideRequestForgery => {
                vec!["csrf".to_string()]
            }
            VulnerabilityClass::CorsMisconfiguration => vec!["cors".to_string()],
            VulnerabilityClass::OpenRedirect => vec!["open-redirect".to_string()],
            VulnerabilityClass::AuthenticationBypass => vec!["authentication-analysis".to_string()],
            VulnerabilityClass::MfaBypass => vec!["mfa-analysis".to_string()],
            VulnerabilityClass::Bfla => vec!["bfla".to_string()],
            VulnerabilityClass::RaceCondition
            | VulnerabilityClass::SessionRaceCondition
            | VulnerabilityClass::RaceConditionFile => vec!["race-condition".to_string()],
            VulnerabilityClass::BusinessLogicFlaw => vec!["business-logic".to_string()],
            VulnerabilityClass::FileUpload => vec!["file-upload".to_string()],
            VulnerabilityClass::InsecureDeserialization => vec!["deserialization".to_string()],
            VulnerabilityClass::Clickjacking => vec!["clickjacking".to_string()],
            VulnerabilityClass::SessionFixation => vec!["session-analysis".to_string()],
            VulnerabilityClass::WeakPasswordPolicy => vec!["authentication-analysis".to_string()],
            VulnerabilityClass::PrivilegeEscalation => vec!["privilege-escalation".to_string()],
            VulnerabilityClass::CrossTenantAccess => vec!["cross-tenant-access".to_string()],
            VulnerabilityClass::PaymentManipulation => vec!["payment-manipulation".to_string()],
            VulnerabilityClass::WorkflowBypass => vec!["workflow-bypass".to_string()],
            VulnerabilityClass::SensitiveDataExposure
            | VulnerabilityClass::InformationDisclosure
            | VulnerabilityClass::ExcessiveDataExposure => {
                vec!["source-leak-analysis".to_string()]
            }
            VulnerabilityClass::Misconfiguration | VulnerabilityClass::SecurityMisconfiguration => {
                vec!["technology-fingerprinting".to_string()]
            }
            VulnerabilityClass::CryptographicWeakness | VulnerabilityClass::WeakCryptography => {
                vec!["crypto-analysis".to_string()]
            }
            VulnerabilityClass::InsecureStorage => vec!["secret-analysis".to_string()],
            VulnerabilityClass::LdapInjection | VulnerabilityClass::LdapInjectionAdvanced => {
                vec!["command-injection".to_string()]
            }
            // Modern vulnerability classes
            VulnerabilityClass::PrototypePollution => vec!["prototype-pollution".to_string()],
            VulnerabilityClass::GraphQlInjection => vec!["graphql".to_string()],
            VulnerabilityClass::JwtAttack => {
                vec!["jwt-analysis".to_string(), "jwt-attack".to_string()]
            }
            VulnerabilityClass::WebsocketHijacking => {
                vec!["websocket".to_string(), "websocket-hijacking".to_string()]
            }
            VulnerabilityClass::SubdomainTakeover => vec!["subdomain-takeover".to_string()],
            VulnerabilityClass::CachePoisoning => vec!["cache-poisoning".to_string()],
            VulnerabilityClass::HttpRequestSmuggling => vec!["http-smuggling".to_string()],
            VulnerabilityClass::OAuthMisconfiguration => vec![
                "oauth-analysis".to_string(),
                "oauth-misconfiguration".to_string(),
            ],
            VulnerabilityClass::SamlInjection => vec!["saml-analysis".to_string()],
            VulnerabilityClass::EsiInjection => vec!["esi-injection".to_string()],
            VulnerabilityClass::HttpResponseSplitting => {
                vec!["http-response-splitting".to_string()]
            }
            VulnerabilityClass::MassAssignment => vec!["mass-assignment".to_string()],
            VulnerabilityClass::DOMClobbering => vec!["dom-clobbering".to_string()],
            VulnerabilityClass::PostMessageVulnerability => vec!["post-message".to_string()],
            VulnerabilityClass::WebCacheDeception => vec!["web-cache-deception".to_string()],
            VulnerabilityClass::HostHeaderInjection => vec!["host-header-injection".to_string()],
            VulnerabilityClass::PasswordResetPoisoning => {
                vec!["password-reset-poisoning".to_string()]
            }
            VulnerabilityClass::EmailHeaderInjection => vec!["email-header-injection".to_string()],
            VulnerabilityClass::UnicodeNormalization => vec!["unicode-normalization".to_string()],
            VulnerabilityClass::BufferOverflow => vec!["buffer-overflow".to_string()],
            VulnerabilityClass::IntegerOverflow => vec!["integer-overflow".to_string()],
            VulnerabilityClass::FormatString => vec!["format-string".to_string()],
            VulnerabilityClass::UseAfterFree => vec!["use-after-free".to_string()],
            VulnerabilityClass::DoubleFree => vec!["double-free".to_string()],
            VulnerabilityClass::SymlinkAttack => vec!["symlink-attack".to_string()],
            VulnerabilityClass::HardcodedCredentials => vec!["hardcoded-credentials".to_string()],
            VulnerabilityClass::InsufficientLogging => vec!["insufficient-logging".to_string()],
            VulnerabilityClass::ImproperInputValidation => vec!["input-validation".to_string()],
            VulnerabilityClass::MissingAuthorization => vec!["missing-authorization".to_string()],
            VulnerabilityClass::BrokenAccessControl => vec!["broken-access-control".to_string()],
            VulnerabilityClass::VulnerableComponents => vec!["vulnerable-components".to_string()],
            VulnerabilityClass::InsufficientMonitoring => {
                vec!["insufficient-monitoring".to_string()]
            }
            VulnerabilityClass::ApiAbuse => vec!["api-abuse".to_string()],
            VulnerabilityClass::RateLimitBypass => vec!["rate-limit-bypass".to_string()],
            VulnerabilityClass::PaginationAbuse => vec!["pagination-abuse".to_string()],
            VulnerabilityClass::FilterBypass => vec!["filter-bypass".to_string()],
            VulnerabilityClass::EncodingBypass => vec!["encoding-bypass".to_string()],
            VulnerabilityClass::WafBypass => vec!["waf-bypass".to_string()],
            VulnerabilityClass::Custom(custom) => {
                let mut skills = vec!["source-code-audit".to_string()];
                // Web3 hypotheses arrive as Custom("reentrancy ...") etc.
                // because VulnerabilityClass has no native EVM variants.
                // Without this, hypothesis-based routing could never select
                // the web3 skill — only technology routing could.
                let lower = custom.to_lowercase();
                const WEB3_KEYWORDS: &[&str] = &[
                    "reentrancy",
                    "reentrance",
                    "oracle",
                    "flash",
                    "vault",
                    "erc4626",
                    "erc20",
                    "erc721",
                    "erc1155",
                    "amm",
                    "uniswap",
                    "lending",
                    "borrow",
                    "collateral",
                    "liquidat",
                    "bridge",
                    "governance",
                    "permit",
                    "eip-712",
                    "eip712",
                    "signature",
                    "replay",
                    "proxy",
                    "uups",
                    "beacon",
                    "diamond",
                    "delegatecall",
                    "solidity",
                    "evm",
                    "defi",
                    "mev",
                    "slippage",
                    "share",
                    "inflation",
                    "donation",
                    "erc",
                    "token",
                    "nft",
                    "dex",
                    "yield",
                    "staking",
                    "farming",
                ];
                if WEB3_KEYWORDS.iter().any(|k| lower.contains(k))
                    && !skills.contains(&"web3-security-research".to_string())
                {
                    skills.push("web3-security-research".to_string());
                }
                skills
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn router_selects_recon_for_small_surface() {
        let scope = LiveScope::new(
            super::super::scope::EngagementMode::BugBounty,
            "test-target".into(),
        );
        let route = SkillRouter::route(&scope, None);
        assert!(route.skills.contains(&"passive-recon".to_string()));
    }

    #[test]
    fn router_selects_ctf_skill_for_ctf_mode() {
        let scope = LiveScope::new(
            super::super::scope::EngagementMode::Ctf,
            "test-target".into(),
        );
        let route = SkillRouter::route(&scope, None);
        assert!(route.skills.contains(&"ctf".to_string()));
    }

    #[test]
    fn router_selects_xss_skill_for_xss_hypothesis() {
        let scope = LiveScope::new(
            super::super::scope::EngagementMode::BugBounty,
            "test-target".into(),
        );
        let h = Hypothesis::new(
            "h1".into(),
            "Reflected XSS".into(),
            VulnerabilityClass::Xss,
            "/search".into(),
            "input reflected".into(),
        );
        let route = SkillRouter::route(&scope, Some(&h));
        assert!(route.skills.contains(&"xss".to_string()));
    }

    #[test]
    fn router_selects_api_for_api_endpoints() {
        let mut scope = LiveScope::new(
            super::super::scope::EngagementMode::BugBounty,
            "test-target".into(),
        );
        scope.add_endpoint(super::super::scope::DiscoveredEndpoint {
            url: "https://test-target/api/users".into(),
            method: "GET".into(),
            path: "/api/users".into(),
            parameters: vec![],
            status_code: Some(200),
            content_type: None,
            requires_auth: true,
            discovered_by: "recon".into(),
            noise_level: super::super::noise::NoiseLevel::Moderate,
            ..Default::default()
        });
        let route = SkillRouter::route(&scope, None);
        assert!(route.skills.contains(&"api-discovery".to_string()));
        assert!(
            route
                .skills
                .contains(&"authentication-analysis".to_string())
        );
    }

    #[test]
    fn skills_for_technology_covers_graphql() {
        let skills = SkillRouter::skills_for_technology("graphql");
        assert!(skills.contains(&"graphql".to_string()));
    }

    #[test]
    fn skills_for_vuln_class_maps_correctly() {
        let skills = SkillRouter::skills_for_vuln_class(&VulnerabilityClass::Ssrf);
        assert!(skills.contains(&"ssrf".to_string()));
    }

    #[test]
    fn custom_web3_hypothesis_routes_to_web3_skill() {
        for label in [
            "ERC4626 donation inflation",
            "oracle manipulation",
            "bridge overmint",
            "governance takeover",
            "reentrancy with value extraction",
        ] {
            let skills =
                SkillRouter::skills_for_vuln_class(&VulnerabilityClass::Custom(label.into()));
            assert!(
                skills.contains(&"web3-security-research".to_string()),
                "hypothesis '{label}' should route to web3-security-research"
            );
        }
        let skills =
            SkillRouter::skills_for_vuln_class(&VulnerabilityClass::Custom("reflected xss".into()));
        assert!(!skills.contains(&"web3-security-research".to_string()));
    }

    #[test]
    fn router_selects_web3_skill_for_solidity_stack() {
        for tech in ["solidity", "evm", "foundry", "erc4626-vault"] {
            let skills = SkillRouter::skills_for_technology(tech);
            assert!(
                skills.contains(&"web3-security-research".to_string()),
                "tech '{tech}' should route to web3-security-research"
            );
        }
        let skills = SkillRouter::skills_for_technology("react");
        assert!(!skills.contains(&"web3-security-research".to_string()));
    }
}
