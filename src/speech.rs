//! Speech-to-text (STT) and text-to-speech (TTS).
//!
//! Supports multiple providers with OpenAI-compatible APIs:
//! - OpenAI (Whisper + TTS)
//! - Groq (Whisper, fast & cheap)
//! - Any OpenAI-compatible endpoint (custom base_url)
//!
//! Provider is configured via SpeechConfig in config.yaml.

#![allow(dead_code)] // TTS path is opt-in via config; synthesize/tts_model kept for future wiring

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Speech provider config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechConfig {
    /// Provider name: "openai", "groq", or "custom"
    #[serde(default = "default_provider")]
    pub provider: String,

    /// API key (or use env: OPENAI_API_KEY / GROQ_API_KEY)
    pub api_key: Option<String>,

    /// Custom base URL (for self-hosted Whisper or other compatible APIs)
    pub base_url: Option<String>,

    /// STT model name (default: "whisper-1" for OpenAI, "whisper-large-v3" for Groq)
    pub stt_model: Option<String>,

    /// TTS model name (default: "tts-1")
    pub tts_model: Option<String>,

    /// TTS voice (default: "alloy")
    #[serde(default = "default_voice")]
    pub tts_voice: String,
}

fn default_provider() -> String {
    "openai".to_string()
}

fn default_voice() -> String {
    "alloy".to_string()
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            api_key: None,
            base_url: None,
            stt_model: None,
            tts_model: None,
            tts_voice: default_voice(),
        }
    }
}

impl SpeechConfig {
    fn resolve_base_url(&self) -> &str {
        if let Some(ref url) = self.base_url {
            return url.as_str();
        }
        match self.provider.as_str() {
            "groq" => "https://api.groq.com/openai/v1",
            _ => "https://api.openai.com/v1",
        }
    }

    fn resolve_api_key(&self) -> Result<String> {
        if let Some(ref key) = self.api_key {
            return Ok(key.clone());
        }
        // Try provider-specific env vars, then fallback
        let env_vars = match self.provider.as_str() {
            "groq" => vec!["GROQ_API_KEY", "OPENAI_API_KEY"],
            _ => vec!["OPENAI_API_KEY"],
        };
        for var in &env_vars {
            if let Ok(key) = std::env::var(var) {
                return Ok(key);
            }
        }
        anyhow::bail!(
            "No API key for speech provider '{}'. Set {} or configure api_key in speech config.",
            self.provider,
            env_vars.join(" / ")
        )
    }

    fn stt_model(&self) -> &str {
        if let Some(ref m) = self.stt_model {
            return m.as_str();
        }
        match self.provider.as_str() {
            "groq" => "whisper-large-v3",
            _ => "whisper-1",
        }
    }

    fn tts_model(&self) -> &str {
        if let Some(ref m) = self.tts_model {
            return m.as_str();
        }
        "tts-1"
    }
}

/// Transcribe an audio file to text.
pub async fn transcribe(config: &SpeechConfig, audio_path: &Path) -> Result<String> {
    let api_key = config.resolve_api_key()?;
    let base_url = config.resolve_base_url();
    let url = format!("{}/audio/transcriptions", base_url);

    // Bounded: this runs on the per-voice-message path; reqwest's default has
    // NO timeout, so a stalled STT endpoint would hang the message forever.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    let file_bytes = tokio::fs::read(audio_path).await?;
    let file_name = audio_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("audio.ogg")
        .to_string();

    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("audio/ogg")?;

    let form = reqwest::multipart::Form::new()
        .text("model", config.stt_model().to_string())
        .part("file", part);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await
        .context("STT API request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("STT API error {}: {}", status, body);
    }

    let body: serde_json::Value = resp.json().await?;
    let text = body["text"].as_str().unwrap_or("").to_string();

    tracing::info!(provider = %config.provider, model = %config.stt_model(), chars = text.len(), "STT: transcribed");
    Ok(text)
}

/// Synthesize text to speech. Returns the path to the generated audio file.
/// Note: Groq does not support TTS — falls back to OpenAI if provider is Groq.
pub async fn synthesize(config: &SpeechConfig, text: &str) -> Result<std::path::PathBuf> {
    // Groq doesn't have TTS, use OpenAI endpoint for TTS regardless
    let (base_url, api_key) = if config.provider == "groq" {
        let key = std::env::var("OPENAI_API_KEY")
            .context("TTS requires OPENAI_API_KEY (Groq does not support TTS)")?;
        ("https://api.openai.com/v1".to_string(), key)
    } else {
        (config.resolve_base_url().to_string(), config.resolve_api_key()?)
    };

    let url = format!("{}/audio/speech", base_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()?;

    let body = serde_json::json!({
        "model": config.tts_model(),
        "input": text,
        "voice": config.tts_voice,
        "response_format": "opus",
    });

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .context("TTS API request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("TTS API error {}: {}", status, body);
    }

    let bytes = resp.bytes().await?;

    let tmp_dir = std::env::temp_dir();
    let filename = format!("agentbridge_tts_{}.opus", uuid::Uuid::new_v4());
    let path = tmp_dir.join(filename);
    tokio::fs::write(&path, &bytes).await?;

    tracing::info!(provider = %config.provider, "TTS: synthesized to {}", path.display());
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_openai() {
        let cfg = SpeechConfig::default();
        assert_eq!(cfg.provider, "openai");
        assert_eq!(cfg.resolve_base_url(), "https://api.openai.com/v1");
        assert_eq!(cfg.stt_model(), "whisper-1");
        assert_eq!(cfg.tts_model(), "tts-1");
    }

    #[test]
    fn groq_config() {
        let cfg = SpeechConfig {
            provider: "groq".to_string(),
            ..Default::default()
        };
        assert_eq!(cfg.resolve_base_url(), "https://api.groq.com/openai/v1");
        assert_eq!(cfg.stt_model(), "whisper-large-v3");
    }

    #[test]
    fn custom_base_url() {
        let cfg = SpeechConfig {
            provider: "custom".to_string(),
            base_url: Some("http://localhost:8080/v1".to_string()),
            ..Default::default()
        };
        assert_eq!(cfg.resolve_base_url(), "http://localhost:8080/v1");
    }

    #[test]
    fn explicit_model_overrides_default() {
        let cfg = SpeechConfig {
            stt_model: Some("whisper-large-v3-turbo".to_string()),
            ..Default::default()
        };
        assert_eq!(cfg.stt_model(), "whisper-large-v3-turbo");
    }
}
