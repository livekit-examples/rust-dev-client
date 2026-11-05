use livekit_thread::{ThreadDirectory, ThreadDirectoryOptions};

/// Settings for browsing the threads of one scope, entered on the connect
/// screen. Threads only support API credentials.
#[derive(Clone)]
pub struct ThreadConnectSettings {
    pub url: String,
    pub api_key: String,
    pub api_secret: String,
    pub directory: ThreadDirectoryOptions,
}

impl From<&ThreadConnectSettings> for ThreadDirectory {
    fn from(settings: &ThreadConnectSettings) -> Self {
        ThreadDirectory::with_api_key(
            &settings.url,
            &settings.api_key,
            &settings.api_secret,
            settings.directory.clone(),
        )
    }
}
