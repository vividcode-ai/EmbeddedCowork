use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::speech::service::SpeechService;

#[derive(Clone)]
pub struct SpeechRouteState {
    pub speech_service: Arc<Mutex<SpeechService>>,
}

#[derive(Debug, Deserialize)]
pub struct SynthesizeRequest {
    pub text: String,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TranscribeRequest {
    pub audio_base64: String,
    pub mime_type: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SynthesizeResponse {
    pub audio_base64: String,
    pub mime_type: String,
}

#[derive(Debug, Serialize)]
pub struct TranscribeResponse {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

pub fn speech_routes(state: SpeechRouteState) -> Router {
    Router::new()
        .route("/api/speech/capabilities", get(get_capabilities))
        .route("/api/speech/voices", get(get_capabilities))
        .route("/api/speech/synthesize", post(synthesize))
        .route("/api/speech/synthesize/stream", post(synthesize_stream))
        .route("/api/speech/transcribe", post(transcribe))
        .with_state(state)
}

async fn get_capabilities(
    State(state): State<SpeechRouteState>,
) -> Json<serde_json::Value> {
    let caps = state.speech_service.lock().await.get_capabilities();
    Json(serde_json::json!({
        "available": true,
        "configured": caps.configured,
        "provider": caps.provider,
        "supportsStt": caps.supports_stt,
        "supportsTts": caps.supports_tts,
        "supportsStreamingTts": false,
        "sttModel": caps.stt_model,
        "ttsModel": caps.tts_model,
        "ttsVoice": caps.tts_voice,
        "ttsFormats": ["mp3", "wav", "opus", "aac"],
        "streamingTtsFormats": [],
    }))
}

async fn synthesize(
    State(state): State<SpeechRouteState>,
    Json(body): Json<SynthesizeRequest>,
) -> Result<Json<SynthesizeResponse>, (StatusCode, Json<serde_json::Value>)> {
    let input = crate::speech::service::SynthesizeSpeechInput {
        text: body.text,
        format: body.format,
    };

    match state.speech_service.lock().await.synthesize(input).await {
        Ok(result) => Ok(Json(SynthesizeResponse {
            audio_base64: result.audio_base64,
            mime_type: result.mime_type,
        })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))),
    }
}

async fn synthesize_stream(
    State(state): State<SpeechRouteState>,
    Json(body): Json<SynthesizeRequest>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let input = crate::speech::service::SynthesizeSpeechInput {
        text: body.text,
        format: body.format,
    };

    let result = state.speech_service.lock().await.synthesize(input).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e}))))?;

    let audio_bytes = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &result.audio_base64,
    ).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to decode audio"}))))?;

    let mime_type = match result.mime_type.as_str() {
        "audio/mpeg" => "audio/mpeg",
        "audio/wav" => "audio/wav",
        "audio/ogg" => "audio/ogg; codecs=\"opus\"",
        "audio/aac" => "audio/aac",
        _ => "audio/mpeg",
    };

    let mut response = Response::new(axum::body::Body::from(audio_bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(mime_type),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

async fn transcribe(
    State(state): State<SpeechRouteState>,
    Json(body): Json<TranscribeRequest>,
) -> Result<Json<TranscribeResponse>, (StatusCode, Json<serde_json::Value>)> {
    let input = crate::speech::service::TranscribeAudioInput {
        audio_base64: body.audio_base64,
        mime_type: body.mime_type,
        filename: body.filename,
        language: body.language,
        prompt: body.prompt,
    };

    match state.speech_service.lock().await.transcribe(input).await {
        Ok(result) => Ok(Json(TranscribeResponse {
            text: result.text,
            language: None,
            duration_ms: None,
        })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e})))),
    }
}
