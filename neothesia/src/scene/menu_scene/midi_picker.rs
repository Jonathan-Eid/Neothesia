use std::path::PathBuf;

use crate::{
    scene::menu_scene::{MsgFn, on_async},
    song::Song,
    utils::BoxFuture,
};

use super::UiState;

pub fn open_midi_file_picker(data: &mut UiState) -> BoxFuture<MsgFn> {
    data.is_loading = true;
    on_async(open_midi_file_picker_fut(), |res, data, ctx| {
        if let Some((midi, path)) = res {
            data.song = Some(Song::with_path(midi, &path));
            ctx.config.set_last_opened_song(Some(path));
        }
        data.is_loading = false;
    })
}

#[cfg(target_os = "android")]
async fn open_midi_file_picker_fut() -> Option<(midi_file::MidiFile, PathBuf)> {
    let path = crate::android::pick_file().await?;

    let thread = crate::utils::task::thread::spawn("midi-loader".into(), move || {
        let midi = midi_file::MidiFile::new(&path);

        if let Err(e) = &midi {
            log::error!("{e}");
        }

        midi.map(|midi| (midi, path)).ok()
    });

    thread.join().await.ok().flatten()
}

#[cfg(not(target_os = "android"))]
async fn open_midi_file_picker_fut() -> Option<(midi_file::MidiFile, PathBuf)> {
    let file = rfd::AsyncFileDialog::new()
        .add_filter("song", &["mid", "midi", "mxl", "musicxml", "xml"])
        .add_filter("midi", &["mid", "midi"])
        .add_filter("musicxml", &["mxl", "musicxml", "xml"])
        .pick_file()
        .await;

    if let Some(file) = file {
        log::info!("File path = {:?}", file.path());

        let thread = crate::utils::task::thread::spawn("midi-loader".into(), move || {
            let midi = midi_file::MidiFile::new(file.path());

            if let Err(e) = &midi {
                log::error!("{e}");
            }

            midi.map(|midi| (midi, file.path().to_path_buf())).ok()
        });

        thread.join().await.ok().flatten()
    } else {
        log::info!("User canceled dialog");
        None
    }
}
