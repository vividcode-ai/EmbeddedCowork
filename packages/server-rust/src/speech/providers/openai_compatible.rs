use async_trait::async_trait;
use base64::engine::general_purpose;
use base64::Engine;
use crate::api_types::{SpeechCapabilitiesResponse, SpeechSynthesisResponse, SpeechTranscriptionResponse};
use crate::logger::Logger;
use crate::log_info;
use crate::speech::service::{NormalizedSpeechSettings, SpeechProvider, SynthesizeSpeechInput, TranscribeAudioInput};

fn mime_to_extension(mime: &str) -> &str {
    match mime {
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/wave" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/flac" => "flac",
        "audio/aac" => "aac",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "audio/webm" => "webm",
        _ => {
            let after_slash = mime.rsplit('/').next().unwrap_or("mp3");
            if after_slash.contains('+') {
                after_slash.rsplit('+').next().unwrap_or("mp3")
            } else {
                after_slash
            }
        }
    }
}

fn format_to_mime(format: &str) -> &str {
    match format {
        "mp3" => "audio/mpeg",
        "wav" | "wave" => "audio/wav",
        "opus" => "audio/opus",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "ogg" => "audio/ogg",
        "wma" => "audio/x-ms-wma",
        _ => "audio/mpeg",
    }
}

pub struct OpenAICompatibleSpeechProvider {
    settings: NormalizedSpeechSettings,
    client: reqwest::Client,
    logger: Logger,
}

impl OpenAICompatibleSpeechProvider {
    pub fn new(settings: NormalizedSpeechSettings, logger: Logger) -> Self {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(ref api_key) = settings.api_key {
            let auth_value = format!("Bearer {}", api_key);
            if let Ok(header_val) = reqwest::header::HeaderValue::from_str(&auth_value) {
                headers.insert(reqwest::header::AUTHORIZATION, header_val);
            }
        }
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { settings, client, logger }
    }

    fn resolve_base_url(&self) -> String {
        self.settings
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
    }

    fn build_api_url(&self, endpoint: &str) -> String {
        let base = self.resolve_base_url();
        let base = base.trim_end_matches('/');
        if let Some(rest) = base.strip_suffix("/v1") {
            format!("{}/v1{}", rest, endpoint)
        } else if base.ends_with("/v1") {
            format!("{}{}", base, endpoint)
        } else {
            format!("{}/v1{}", base, endpoint)
        }
    }
}

#[async_trait]
impl SpeechProvider for OpenAICompatibleSpeechProvider {
    fn get_capabilities(&self) -> SpeechCapabilitiesResponse {
        let base_url = self.settings.base_url.clone();
        SpeechCapabilitiesResponse {
            available: self.settings.api_key.is_some(),
            configured: self.settings.api_key.is_some(),
            provider: self.settings.provider.clone(),
            supports_stt: true,
            supports_tts: true,
            supports_streaming_tts: true,
            base_url,
            stt_model: self.settings.stt_model.clone(),
            tts_model: self.settings.tts_model.clone(),
            tts_voice: self.settings.tts_voice.clone(),
            tts_formats: vec!["mp3".to_string(), "wav".to_string(), "opus".to_string(), "aac".to_string()],
            streaming_tts_formats: vec!["mp3".to_string()],
        }
    }

    async fn transcribe(&self, input: TranscribeAudioInput) -> Result<SpeechTranscriptionResponse, String> {
        let url = self.build_api_url("/audio/transcriptions");

        let audio_bytes = general_purpose::STANDARD
            .decode(&input.audio_base64)
            .map_err(|e| format!("Failed to decode base64 audio data: {}", e))?;

        let extension = mime_to_extension(&input.mime_type);
        let mime_type = if input.mime_type.is_empty() {
            "audio/mpeg"
        } else {
            &input.mime_type
        };

        let file_part = reqwest::multipart::Part::bytes(audio_bytes)
            .file_name(format!("audio.{}", extension))
            .mime_str(mime_type)
            .map_err(|e| format!("Failed to set MIME type on file part: {}", e))?;

        let mut form = reqwest::multipart::Form::new()
            .part("file", file_part)
            .text("model", self.settings.stt_model.clone());

        if let Some(ref lang) = input.language {
            form = form.text("language", lang.clone());
        }
        if let Some(ref prompt) = input.prompt {
            form = form.text("prompt", prompt.clone());
        }

        let response = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Transcription HTTP request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(format!(
                "Transcription API returned HTTP {}: {}",
                status.as_u16(),
                body_text
            ));
        }

        #[derive(serde::Deserialize)]
        struct TranscriptionResponse {
            text: String,
        }

        let transcription: TranscriptionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse transcription response: {}", e))?;

        log_info!(self.logger, format!("Transcription completed: text_length={}, model={}", transcription.text.len(), self.settings.stt_model));

        Ok(SpeechTranscriptionResponse {
            text: transcription.text,
            language: None,
            duration_ms: None,
            segments: None,
        })
    }

    async fn synthesize(&self, input: SynthesizeSpeechInput) -> Result<SpeechSynthesisResponse, String> {
        let url = self.build_api_url("/audio/speech");

        let format = input
            .format
            .unwrap_or_else(|| self.settings.tts_format.clone());

        #[derive(serde::Serialize)]
        struct SynthesisRequest<'a> {
            model: &'a str,
            input: &'a str,
            voice: &'a str,
            response_format: &'a str,
        }

        let body = SynthesisRequest {
            model: &self.settings.tts_model,
            input: &input.text,
            voice: &self.settings.tts_voice,
            response_format: &format,
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Synthesis HTTP request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(format!(
                "Synthesis API returned HTTP {}: {}",
                status.as_u16(),
                body_text
            ));
        }

        let audio_bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Failed to read synthesis response body: {}", e))?;

        let audio_base64 = general_purpose::STANDARD.encode(&audio_bytes);
        let mime_type = format_to_mime(&format).to_string();

        log_info!(self.logger, format!("Synthesis completed: byte_length={}, format={}, model={}", audio_bytes.len(), format, self.settings.tts_model));

        Ok(SpeechSynthesisResponse {
            audio_base64,
            mime_type,
        })
    }
}
