use std::sync::Arc;
use tokio::sync::Mutex;

use crate::plugins::channel::PluginChannelManager;
use crate::plugins::voice_mode::VoiceModeManager;

pub struct PluginHandlers {
    pub channel: Arc<Mutex<PluginChannelManager>>,
    pub voice_mode: Arc<Mutex<VoiceModeManager>>,
}
