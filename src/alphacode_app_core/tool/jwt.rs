use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct JwtTool;

#[derive(Deserialize)]
struct JwtInput {
    action: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    secret: Option<String>,
    #[serde(default)]
    algorithm: Option<String>,
    #[serde(default)]
    payload: Option<String>,
    #[serde(default)]
    header: Option<String>,
    #[serde(default)]
    wordlist: Option<String>,
    #[serde(default)]
    kid: Option<String>,
    #[serde(default)]
    claim_key: Option<String>,
    #[serde(default)]
    claim_value: Option<String>,
}

#[async_trait]
impl Tool for JwtTool {
    fn name(&self) -> &str {
        "jwt"
    }

    fn description(&self) -> &str {
        "JWT decode, crack, forge, and manipulate. Decode tokens, crack weak secrets \
         with wordlists, forge new tokens with custom claims, perform algorithm confusion \
         attacks, and inject kid parameters. Essential for web security testing."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ["decode", "crack", "forge", "manipulate", "forge_secret"],
                    "description": "Action: decode=parse token, crack=find secret with wordlist, forge=create token, manipulate=modify existing, forge_secret=sign with custom secret."
                },
                "token": {
                    "type": "string",
                    "description": "JWT token to decode/crack/manipulate."
                },
                "secret": {
                    "type": "string",
                    "description": "Secret key for cracking or signing."
                },
                "algorithm": {
                    "type": "string",
                    "enum": ["HS256", "HS384", "HS512", "RS256", "RS384", "RS512", "none"],
                    "description": "JWT algorithm (default: auto-detect or HS256)."
                },
                "payload": {
                    "type": "string",
                    "description": "Custom payload JSON for forge action."
                },
                "header": {
                    "type": "string",
                    "description": "Custom header JSON for forge action (alg, kid, jku, etc)."
                },
                "wordlist": {
                    "type": "string",
                    "description": "Path to wordlist file for crack action (one secret per line)."
                },
                "kid": {
                    "type": "string",
                    "description": "Key ID to inject into header (for kid injection attacks)."
                },
                "claim_key": {
                    "type": "string",
                    "description": "Claim key to modify in manipulate action."
                },
                "claim_value": {
                    "type": "string",
                    "description": "New value for claim in manipulate action."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: JwtInput = serde_json::from_value(input)?;

        match params.action.as_str() {
            "decode" => self.decode_token(&params),
            "crack" => self.crack_token(&params).await,
            "forge" => self.forge_token(&params),
            "manipulate" => self.manipulate_token(&params),
            "forge_secret" => self.forge_with_secret(&params),
            _ => Err(anyhow::anyhow!(
                "Unknown action: {}. Use decode, crack, forge, manipulate, or forge_secret.",
                params.action
            )),
        }
    }
}

impl JwtTool {
    fn decode_token(&self, params: &JwtInput) -> Result<ToolOutput> {
        let token = params
            .token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("token is required for decode"))?;

        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!(
                "Invalid JWT: expected 3 parts, got {}. Format: header.payload.signature",
                parts.len()
            ));
        }

        let header = decode_base64_url(parts[0])?;
        let payload = decode_base64_url(parts[1])?;

        let mut output = String::new();
        output.push_str("=== HEADER ===\n");
        output.push_str(&header);
        output.push_str("\n\n=== PAYLOAD ===\n");
        output.push_str(&payload);
        output.push_str("\n\n=== SIGNATURE ===\n");
        output.push_str(parts[2]);

        // Parse and highlight key claims
        if let Ok(payload_json) = serde_json::from_str::<Value>(&payload) {
            output.push_str("\n\n=== KEY CLAIMS ===\n");
            for key in [
                "sub",
                "iss",
                "exp",
                "iat",
                "aud",
                "role",
                "admin",
                "privilege",
            ] {
                if let Some(val) = payload_json.get(key) {
                    output.push_str(&format!("{}: {}\n", key, val));
                }
            }
            // Check expiration
            if let Some(exp) = payload_json.get("exp").and_then(|v| v.as_u64()) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if exp < now {
                    output.push_str("\n[EXPIRED TOKEN]\n");
                } else {
                    output.push_str(&format!("\nExpires in: {} seconds\n", exp - now));
                }
            }
        }

        Ok(ToolOutput::new(output))
    }

    async fn crack_token(&self, params: &JwtInput) -> Result<ToolOutput> {
        let token = params
            .token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("token is required for crack"))?;

        let wordlist_path = params
            .wordlist
            .as_deref()
            .unwrap_or("/usr/share/wordlists/rockyou.txt");

        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid JWT format"));
        }

        // Read wordlist
        let words = if let Ok(content) = tokio::fs::read_to_string(wordlist_path).await {
            content
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
        } else {
            // Fallback: common secrets
            vec![
                "secret".to_string(),
                "password".to_string(),
                "key".to_string(),
                "jwt_secret".to_string(),
                "changeme".to_string(),
                "123456".to_string(),
                "test".to_string(),
                "admin".to_string(),
                "secret1".to_string(),
                "supersecret".to_string(),
            ]
        };

        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let expected_sig = parts[2];

        // Try each candidate
        for candidate in &words {
            if let Ok(sig) = sign_hs256(&signing_input, candidate)
                && sig == expected_sig
            {
                return Ok(ToolOutput::new(format!(
                    "FOUND SECRET: {}\n\nAlgorithm: HS256\nSigning input length: {} chars",
                    candidate,
                    signing_input.len()
                )));
            }
        }

        Ok(ToolOutput::new(format!(
            "No matching secret found.\nTried {} candidates.\n\nTip: Provide a custom wordlist with the wordlist parameter.",
            words.len()
        )))
    }

    fn forge_token(&self, params: &JwtInput) -> Result<ToolOutput> {
        let payload = params
            .payload
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("payload is required for forge"))?;

        let algorithm = params.algorithm.as_deref().unwrap_or("HS256");

        let header_json = if let Some(h) = &params.header {
            serde_json::from_str::<Value>(h)?
        } else {
            let mut h = json!({"alg": algorithm, "typ": "JWT"});
            if let Some(kid) = &params.kid {
                h["kid"] = json!(kid);
            }
            h
        };

        let header_b64 = encode_base64_url(&header_json.to_string());
        let payload_b64 = encode_base64_url(payload);

        let signing_input = format!("{}.{}", header_b64, payload_b64);

        let signature = if algorithm == "none" {
            String::new()
        } else if algorithm.starts_with("HS") {
            let secret = params.secret.as_deref().unwrap_or("secret");
            sign_hs256(&signing_input, secret)?
        } else {
            return Err(anyhow::anyhow!(
                "Algorithm {} requires a private key. Use forge_secret with a PEM key.",
                algorithm
            ));
        };

        let token = format!("{}.{}.{}", header_b64, payload_b64, signature);

        let mut output = format!("=== FORGED TOKEN ===\n{}\n\n", token);
        output.push_str("=== HEADER ===\n");
        output.push_str(&serde_json::to_string_pretty(&header_json)?);
        output.push_str("\n\n=== PAYLOAD ===\n");
        output.push_str(payload);
        output.push_str(&format!("\n\n=== ALGORITHM ===\n{}", algorithm));

        Ok(ToolOutput::new(output))
    }

    fn manipulate_token(&self, params: &JwtInput) -> Result<ToolOutput> {
        let token = params
            .token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("token is required for manipulate"))?;

        let claim_key = params
            .claim_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("claim_key is required for manipulate"))?;

        let claim_value = params
            .claim_value
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("claim_value is required for manipulate"))?;

        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid JWT format"));
        }

        let mut payload: Value = serde_json::from_str(&decode_base64_url(parts[1])?)?;

        // Parse the claim value (try JSON first, then string)
        let new_value = if let Ok(v) = serde_json::from_str::<Value>(claim_value) {
            v
        } else {
            json!(claim_value)
        };

        payload[claim_key] = new_value;

        let header = decode_base64_url(parts[0])?;
        let header_json: Value = serde_json::from_str(&header)?;
        let algorithm = header_json
            .get("alg")
            .and_then(|v| v.as_str())
            .unwrap_or("HS256");

        let header_b64 = encode_base64_url(&header);
        let payload_b64 = encode_base64_url(&payload.to_string());

        let signing_input = format!("{}.{}", header_b64, payload_b64);

        let signature = if algorithm == "none" {
            String::new()
        } else if algorithm.starts_with("HS") {
            let secret = params.secret.as_deref().unwrap_or("secret");
            sign_hs256(&signing_input, secret)?
        } else {
            return Err(anyhow::anyhow!(
                "Algorithm {} requires a private key for signing.",
                algorithm
            ));
        };

        let new_token = format!("{}.{}.{}", header_b64, payload_b64, signature);

        let mut output = format!("=== MANIPULATED TOKEN ===\n{}\n\n", new_token);
        output.push_str(&format!(
            "=== MODIFIED CLAIM ===\n{}: {}\n\n",
            claim_key, claim_value
        ));
        output.push_str("=== UPDATED PAYLOAD ===\n");
        output.push_str(&serde_json::to_string_pretty(&payload)?);

        Ok(ToolOutput::new(output))
    }

    fn forge_with_secret(&self, params: &JwtInput) -> Result<ToolOutput> {
        let payload = params
            .payload
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("payload is required"))?;

        let secret = params
            .secret
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("secret is required for forge_secret"))?;

        let algorithm = params.algorithm.as_deref().unwrap_or("HS256");

        let mut header_json = json!({"alg": algorithm, "typ": "JWT"});
        if let Some(kid) = &params.kid {
            header_json["kid"] = json!(kid);
        }

        let header_b64 = encode_base64_url(&header_json.to_string());
        let payload_b64 = encode_base64_url(payload);
        let signing_input = format!("{}.{}", header_b64, payload_b64);

        let signature = match algorithm {
            "HS256" => sign_hs256(&signing_input, secret)?,
            "HS384" => sign_hs384(&signing_input, secret)?,
            "HS512" => sign_hs512(&signing_input, secret)?,
            "none" => String::new(),
            _ => {
                return Err(anyhow::anyhow!(
                    "Algorithm {} not supported for symmetric signing.",
                    algorithm
                ));
            }
        };

        let token = format!("{}.{}.{}", header_b64, payload_b64, signature);

        Ok(ToolOutput::new(format!(
            "=== SIGNED TOKEN ({}) ===\n{}\n\n=== SECRET ===\n{}",
            algorithm, token, secret
        )))
    }
}

fn decode_base64_url(input: &str) -> Result<String> {
    use base64::Engine;
    let mut padded = input.to_string();
    while !padded.len().is_multiple_of(4) {
        padded.push('=');
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&padded)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(&padded))?;
    Ok(String::from_utf8_lossy(&decoded).to_string())
}

fn encode_base64_url(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input.as_bytes())
}

fn sign_hs256(data: &str, secret: &str) -> Result<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("HMAC error: {}", e))?;
    mac.update(data.as_bytes());
    let result = mac.finalize();
    Ok(encode_base64_url(&String::from_utf8_lossy(
        &result.into_bytes(),
    )))
}

fn sign_hs384(data: &str, secret: &str) -> Result<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha384;
    type HmacSha384 = Hmac<Sha384>;
    let mut mac = HmacSha384::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("HMAC error: {}", e))?;
    mac.update(data.as_bytes());
    let result = mac.finalize();
    Ok(encode_base64_url(&String::from_utf8_lossy(
        &result.into_bytes(),
    )))
}

fn sign_hs512(data: &str, secret: &str) -> Result<String> {
    use hmac::{Hmac, Mac};
    use sha2::Sha512;
    type HmacSha512 = Hmac<Sha512>;
    let mut mac = HmacSha512::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("HMAC error: {}", e))?;
    mac.update(data.as_bytes());
    let result = mac.finalize();
    Ok(encode_base64_url(&String::from_utf8_lossy(
        &result.into_bytes(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_jwt() {
        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header = decode_base64_url(parts[0]).unwrap();
        assert!(header.contains("HS256"));
    }

    #[test]
    fn test_encode_base64_url() {
        let encoded = encode_base64_url("hello");
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }
}
