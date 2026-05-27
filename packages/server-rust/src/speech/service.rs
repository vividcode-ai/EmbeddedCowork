use async_trait::async_trait;
use crate::api_types::{SpeechCapabilitiesResponse, SpeechSynthesisResponse, SpeechTranscriptionResponse};
use crate::logger::Logger;
use crate::settings::service::{DocKind, SettingsService};
use crate::speech::providers::openai_compatible::OpenAICompatibleSpeechProvider;

const DEFAULT_PROVIDER: &str = "openai-compatible";
const DEFAULT_STT_MODEL: &str = "gpt-4o-mini-transcribe";
const DEFAULT_TTS_MODEL: &str = "gpt-4o-mini-tts";
const DEFAULT_TTS_VOICE: &str = "alloy";
const DEFAULT_TTS_FORMAT: &str = "mp3";

#[derive(Debug, Clone)]
pub struct TranscribeAudioInput {
    pub audio_base64: String,
    pub mime_type: String,
    pub filename: Option<String>,
    pub language: Option<String>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SynthesizeSpeechInput {
    pub text: String,
    pub format: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NormalizedSpeechSettings {
    pub provider: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub stt_model: String,
    pub tts_model: String,
    pub tts_voice: String,
    pub tts_format: String,
}

#[async_trait]
pub trait SpeechProvider {
    fn get_capabilities(&self) -> SpeechCapabilitiesResponse;
    async fn transcribe(&self, input: TranscribeAudioInput) -> Result<SpeechTranscriptionResponse, String>;
    async fn synthesize(&self, input: SynthesizeSpeechInput) -> Result<SpeechSynthesisResponse, String>;
}

pub struct SpeechService {
    settings: SettingsService,
    logger: Logger,
}

impl SpeechService {
    pub fn new(settings: SettingsService, logger: Logger) -> Self {
        Self { settings, logger }
    }

    pub fn get_capabilities(&mut self) -> SpeechCapabilitiesResponse {
        self.create_provider().get_capabilities()
    }

    pub async fn transcribe(&mut self, input: TranscribeAudioInput) -> Result<SpeechTranscriptionResponse, String> {
        self.create_provider().transcribe(input).await
    }

    pub async fn synthesize(&mut self, input: SynthesizeSpeechInput) -> Result<SpeechSynthesisResponse, String> {
        self.create_provider().synthesize(input).await
    }

    fn create_provider(&mut self) -> OpenAICompatibleSpeechProvider {
        let settings = self.resolve_settings();
        let provider_name = settings.provider.clone();
        OpenAICompatibleSpeechProvider::new(
            settings,
            self.logger.child(&provider_name),
        )
    }

    fn resolve_settings(&mut self) -> NormalizedSpeechSettings {
        let server = self.settings.get_owner(&DocKind::Config, "server");
        let speech = server.get("speech");

        let provider = speech
            .and_then(|s| s.get("provider"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_PROVIDER.to_string());

        let api_key = speech
            .and_then(|s| s.get("apiKey"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok());

        let base_url = speech
            .and_then(|s| s.get("baseUrl"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("OPENAI_BASE_URL").ok());

        let stt_model = speech
            .and_then(|s| s.get("sttModel"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_STT_MODEL.to_string());

        let tts_model = speech
            .and_then(|s| s.get("ttsModel"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_TTS_MODEL.to_string());

        let tts_voice = speech
            .and_then(|s| s.get("ttsVoice"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_TTS_VOICE.to_string());

        let tts_format = speech
            .and_then(|s| s.get("ttsFormat"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_TTS_FORMAT.to_string());

        NormalizedSpeechSettings {
            provider,
            api_key,
            base_url,
            stt_model,
            tts_model,
            tts_voice,
            tts_format,
        }
    }
}
