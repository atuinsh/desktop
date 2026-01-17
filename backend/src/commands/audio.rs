use rodio::{Decoder, OutputStream, Sink};
use serde::Serialize;
use std::fs::File;
use std::io::BufReader;
use tauri::{path::BaseDirectory, AppHandle, Manager, Runtime};

#[derive(Debug, Serialize)]
pub struct SoundInfo {
    /// The filename without extension (e.g., "glad_to_know")
    pub id: String,
    /// Human-readable name (e.g., "Glad To Know")
    pub name: String,
}

/// Convert a filename stem (with underscores) to a human-readable name
fn to_display_name(stem: &str) -> String {
    stem.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[tauri::command]
pub async fn list_sounds<R: Runtime>(app: AppHandle<R>) -> Result<Vec<SoundInfo>, String> {
    let sounds_path = app
        .path()
        .resolve("resources/sounds", BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;

    let mut sounds = Vec::new();

    let entries = std::fs::read_dir(&sounds_path).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        // Only include .ogg files
        if path.extension().and_then(|s| s.to_str()) != Some("ogg") {
            continue;
        }

        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            let name = to_display_name(stem);
            sounds.push(SoundInfo {
                id: stem.to_string(),
                name,
            });
        }
    }

    // Sort by name for consistent ordering
    sounds.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(sounds)
}

#[tauri::command]
pub async fn play_sound<R: Runtime>(
    app: AppHandle<R>,
    sound_id: String,
    volume: f32,
) -> Result<(), String> {
    log::info!(
        "play_sound called with sound_id={}, volume={}",
        sound_id,
        volume
    );

    let sounds_dir = app
        .path()
        .resolve("resources/sounds", BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;

    let sound_path = sounds_dir.join(format!("{}.ogg", sound_id));
    if !sound_path.exists() {
        return Err(format!("Sound not found: {}", sound_id));
    }

    // Clamp volume to valid range
    let volume = volume.clamp(0.0, 1.0);
    log::info!("Playing sound {} at volume {}", sound_id, volume);

    // Spawn audio playback in a separate thread to avoid blocking
    std::thread::spawn(move || {
        let file = match File::open(&sound_path) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("Failed to open sound file {:?}: {}", sound_path, e);
                return;
            }
        };

        let reader = BufReader::new(file);

        let (_stream, stream_handle) = match OutputStream::try_default() {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to get audio output stream: {}", e);
                return;
            }
        };

        let sink = match Sink::try_new(&stream_handle) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to create audio sink: {}", e);
                return;
            }
        };

        // Set volume before appending source
        sink.set_volume(volume);

        let source = match Decoder::new(reader) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Failed to decode sound file: {}", e);
                return;
            }
        };

        sink.append(source);
        sink.sleep_until_end();
    });

    Ok(())
}
